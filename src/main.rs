use chrono::{NaiveDate, Utc, Datelike, NaiveDateTime};
use log::info;
use teloxide::{prelude::*, utils::command::BotCommands};
use chrono::Duration;
use std::collections::HashMap;
use std::error::Error;
use std::{env, thread};
use once_cell::sync::Lazy;
use std::sync::{Mutex, MutexGuard};
use serde::{Deserialize, Serialize};
use tokio_cron_scheduler::{JobScheduler, Job};
use dotenv::dotenv;

extern crate pretty_env_logger;
#[macro_use] extern crate log;

const CURRENT_TASKS_FILE_NAME: &str = "current_tasks.json";
const ALL_TASKS_FILE_NAME: &str = "all_tasks.json";
const TASK_QUEUE_FILE_NAME: &str = "task_que.json";
const POINTS_FILE_NAME: &str = "points.json";
const COMPLETED_TASKS_FILE_NAME: &str = "completed_tasks.json";

static CURRENT_TASKS: Lazy<Mutex<Vec<Task>>> = Lazy::new(|| {
    match serde_any::from_file(CURRENT_TASKS_FILE_NAME.to_string()) {
        Ok(hm) => Mutex::new(hm),
        Err(_) => Mutex::new(vec![])
    }
});

static ALL_TASKS: Lazy<Mutex<Vec<Task>>> = Lazy::new(|| {
    match serde_any::from_file(ALL_TASKS_FILE_NAME.to_string()) {
        Ok(hm) => Mutex::new(hm),
        Err(_) => Mutex::new(vec![])
    }
});

static TASK_QUEUE: Lazy<Mutex<Vec<QueTask>>> = Lazy::new(|| {
    match serde_any::from_file(TASK_QUEUE_FILE_NAME.to_string()) {
        Ok(hm) => Mutex::new(hm),
        Err(_) => Mutex::new(vec![])
    }
});

static COMPLETED_TASKS: Lazy<Mutex<Vec<QueTask>>> = Lazy::new(|| {
    match serde_any::from_file(COMPLETED_TASKS_FILE_NAME.to_string()) {
        Ok(hm) => Mutex::new(hm),
        Err(_) => Mutex::new(vec![])
    }
});

static POINTS: Lazy<Mutex<HashMap<i32, i64>>> = Lazy::new(|| {
    match serde_any::from_file(POINTS_FILE_NAME.to_string()) {
        Ok(hm) => Mutex::new(hm),
        Err(_) => Mutex::new(HashMap::new())
    }
});

#[derive(Debug, Clone, Deserialize, Serialize)]
struct QueTask {
    date: NaiveDate,
    task_index: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct Task {
    points: i8,
    label: String,
    day_interval: i8,
}

#[derive(BotCommands, Clone)]
#[command(rename = "lowercase", description = "These commands are supported:")]
enum Command {
    #[command(description = "Display this text.")]
    Help,
    #[command(description = "Claim the task as done. This will award you the task points.")]
    Claim(i8),
    #[command(description = "Remove item from list if it does not need to be done: /pass <item number>")]
    Pass(i8),
    #[command(description = "List all items to be completed")]
    List,
    #[command(description = "Display the current score.")]
    Score,
}

#[tokio::main]
async fn main() {
    dotenv().ok();
    let token = env::var("TELOXIDE_TOKEN").expect("$TELOXIDE_TOKEN is not set");
    env::set_var("TELOXIDE_TOKEN", token);
    pretty_env_logger::init();
    init_queue();
    let bot = Bot::from_env().auto_send();
    thread::spawn(|| {
        run_cron();
    });
    info!("Running telegram bot!");
    teloxide::commands_repl(bot, answer, Command::ty()).await;
}

#[tokio::main]
async fn run_cron() {
    let mut sched = JobScheduler::new();

    // add job for handling queue
    // the job should add items from queue to current tasks
    // the job should add new que items to the queue
    match sched.add(Job::new_async("0 10,20,23,25,30,33,40,50,0 * * * *", move |_, _|  Box::pin(async { 
        match refiresh_tasks().await {
            Ok(_) => (),
            Err(e) => error!("Error on refreshing tasks: {:?}", e)
        }
    })).unwrap()) {
        Ok(c) => info!("Started cron!: {:?}", c),
        Err(e) => error!("Something went wrong scheduling CRON: {:?}", e)
    };

    // add job for morning reminders of current tasks for the day


    // set shudown handler
    match sched.set_shutdown_handler(Box::new(|| {
        Box::pin(async move {
          info!("Shut down done");
        })
    })) {
        Ok(c) => info!("Shutdown handler set for cron!: {:?}", c),
        Err(e) => error!("Something went wrong setting shutdown handler for CRON: {:?}", e)
    };

    // start cron
    if let Err(e) = sched.start().await {
        error!("Error on scheduler {:?}", e);
    }
}


async fn refiresh_tasks() -> Result<(), Box<dyn Error + Send + Sync>> {
    let today = ndt_to_nd(Utc::now().naive_utc());
    
    // claim mutexes
    let all_tasks = ALL_TASKS.lock().unwrap();
    let mut queue = TASK_QUEUE.lock().unwrap();
    let mut current_tasks = CURRENT_TASKS.lock().unwrap();

    // check if que is broken (if file deleted from disk or smth)
    if queue.len() != all_tasks.len() {
        fill_que(&all_tasks, &mut queue);
    }

    // refresh que (queue -> current)
    for task in queue.iter_mut() {
        if today.signed_duration_since(task.date).num_seconds() >= 0 {
            // if it's time for the task, put it into current tasks
            // first check if it already exists tho...
            if let None = current_tasks.iter().position(|r| r.label == all_tasks[task.task_index].label) {
                current_tasks.push(all_tasks[task.task_index].clone());
            }
     
            // schedule task to next occurance in queue
            // if somehow fails it does not modify the queue (will trigger again tommorow)
            warn!("DATE: {:?} -> {:?}", task.date, task.date.checked_add_signed(Duration::days(all_tasks[task.task_index].day_interval as i64)));
            task.date = match task.date.checked_add_signed(Duration::days(all_tasks[task.task_index].day_interval as i64)) {
                Some(dt) => dt,
                None => {
                    error!("failed to move task in queue");
                    task.date
                }
            };
        }
    }

    //save changes to files
    match serde_any::to_file(TASK_QUEUE_FILE_NAME, &*queue) {
        Ok(_) => {},
        Err(e) => {error!("Error saving task queue: {:?}", e);}
    };
    match serde_any::to_file(CURRENT_TASKS_FILE_NAME, &*current_tasks) {
        Ok(_) => {},
        Err(e) => {error!("Error saving curret tasks: {:?}", e);}
    };
    Ok(())   
}

async fn answer(
    bot: AutoSend<Bot>,
    message: Message,
    command: Command,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    match command {
        Command::Help                   => { bot.send_message(message.chat.id, Command::descriptions().to_string()).await? },
        Command::Claim(item_id)     => { bot.send_message(message.chat.id, claim_task(&bot, message, item_id)).await? },
        Command::Pass(item_id)      => { bot.send_message(message.chat.id, pass_task(&bot, message, item_id)).await? },
        Command::List                   => { bot.send_message(message.chat.id, list_tasks(&bot, message)).await? },
        Command::Score                  => { bot.send_message(message.chat.id, display_score(&bot, message)).await? },
    };
    Ok(())
}

fn claim_task(
    _: &AutoSend<Bot>,
    _message: Message,
    _item_id: i8
) -> String {
    "TODO".to_string()
}

fn pass_task(
    _: &AutoSend<Bot>,
    _message: Message,
    _item_id: i8
) -> String {
    "TODO".to_string()
}

fn list_tasks(
    _: &AutoSend<Bot>,
    _message: Message,
) -> String {
    "TODO".to_string()
}

fn display_score(
    _: &AutoSend<Bot>,
    _message: Message,
) -> String {
    "TODO".to_string()
}

fn init_queue() -> () {
    let all_tasks = ALL_TASKS.lock().unwrap();
    let mut queue = TASK_QUEUE.lock().unwrap();
    if queue.len() != all_tasks.len() {
        warn!("Fixing queue! Missing tasks: {}", all_tasks.len() - queue.len());
        fill_que(&all_tasks, &mut queue);
    } else {
        info!("Queue is all good! :)");
    }
    match serde_any::to_file(TASK_QUEUE_FILE_NAME, &*queue) {
        Ok(_) => {},
        Err(e) => {error!("Error saving task queue: {:?}", e);}
    };
}

fn fill_que(
    all_tasks: &MutexGuard<Vec<Task>>, 
    queue: &mut MutexGuard<Vec<QueTask>>
) -> () {
    let today_option = Utc::now().naive_utc().checked_sub_signed(Duration::days(1));
    for task in all_tasks.iter() {
        if let Some(today_ndt) = today_option {
            let today = ndt_to_nd(today_ndt);
            if let Some(task_date) = today.checked_add_signed(Duration::days(task.day_interval as i64)) {
                // find index of task to add
                // if task does not exist (always sould), skip to next task
                let task_index_option = all_tasks.iter().position(|r| r.label == task.label);
                let task_index = match task_index_option {
                    Some(task_index) => task_index,
                    None => continue
                };
                // if task not already in que, add it
                // otherwise skip to next task
                let que_index_option = queue.iter().position(|r| r.task_index == task_index);
                match que_index_option {
                    Some(_) => continue,
                    None => queue.push(QueTask {
                        date: task_date,
                        task_index: task_index,
                    })
                };
                
            }
        }
    }
}


fn ndt_to_nd(ndt: NaiveDateTime) -> NaiveDate {
    NaiveDate::from_ymd(
        ndt.year(), 
        ndt.month(), 
        ndt.day()
    )
}