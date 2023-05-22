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
const NAMES_FILE_NAME: &str = "names.json";
const COMPLETED_TASKS_FILE_NAME: &str = "completed_tasks.json";

static CURRENT_TASKS: Lazy<Mutex<Vec<Task>>> = Lazy::new(|| {
    match serde_any::from_file(CURRENT_TASKS_FILE_NAME) {
        Ok(hm) => Mutex::new(hm),
        Err(_) => Mutex::new(vec![])
    }
});

static ALL_TASKS: Lazy<Mutex<Vec<Task>>> = Lazy::new(|| {
    match serde_any::from_file(ALL_TASKS_FILE_NAME) {
        Ok(hm) => Mutex::new(hm),
        Err(_) => Mutex::new(vec![])
    }
});

static TASK_QUEUE: Lazy<Mutex<Vec<QueTask>>> = Lazy::new(|| {
    match serde_any::from_file(TASK_QUEUE_FILE_NAME) {
        Ok(hm) => Mutex::new(hm),
        Err(_) => Mutex::new(vec![])
    }
});

static COMPLETED_TASKS: Lazy<Mutex<HashMap<UserId, Vec<QueTask>>>> = Lazy::new(|| {
    match serde_any::from_file(COMPLETED_TASKS_FILE_NAME) {
        Ok(hm) => Mutex::new(hm),
        Err(_) => Mutex::new(HashMap::new())
    }
});

static NAMES: Lazy<Mutex<HashMap<UserId, String>>> = Lazy::new(|| {
    match serde_any::from_file(NAMES_FILE_NAME) {
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
    Claim(String),
    #[command(description = "Remove item from list if it does not need to be done: /pass <item number>")]
    Pass(i8),
    #[command(description = "List all items to be completed")]
    List,
    #[command(description = "Display the current score.")]
    Score,
}

#[tokio::main]
async fn main() {
    // setup env variables
    dotenv().ok();
    env::set_var("TELOXIDE_TOKEN", env::var("TELOXIDE_TOKEN").expect("$TELOXIDE_TOKEN is not set"));
    env::set_var("CHAT_ID", env::var("CHAT_ID").expect("$CHAT_ID is not set"));
    
    // init stuff
    pretty_env_logger::init();
    init_queue();

    // run bot and CRON thread
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
    match sched.add(Job::new_async("0 0 0 * * *", move |_, _|  Box::pin(async { 
        match refresh_tasks().await {
            Ok(_) => (),
            Err(e) => error!("Error on refreshing tasks: {:?}", e)
        }
    })).unwrap()) {
        Ok(c) => info!("Started cron!: {:?}", c),
        Err(e) => error!("Something went wrong scheduling CRON: {:?}", e)
    };

    // add job for morning reminders of current tasks for the day
    match sched.add(Job::new_async("0 0 7 * * *", move |_, _|  Box::pin(async { 
        match notify().await {
            Ok(_) => (),
            Err(e) => error!("Error on refreshing tasks: {:?}", e)
        }
    })).unwrap()) {
        Ok(c) => info!("Started cron!: {:?}", c),
        Err(e) => error!("Something went wrong scheduling CRON: {:?}", e)
    };

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


async fn notify() -> Result<(), Box<dyn Error + Send + Sync>> {
    info!("Notifying chat of daily tasks!");
    tokio::task::spawn(async move {
        match Bot::from_env().auto_send().send_message(
            env::var("CHAT_ID").expect("$CHAT_ID is not set"),
            construct_notification_text()
        ).await {
            Ok(_) => (),
            Err(e) => error!("{:?}", e),
        };        
    });
    Ok(())
}

fn construct_notification_text() -> String {
    let mut current_tasks = CURRENT_TASKS.lock().unwrap();
    let all_tasks = ALL_TASKS.lock().unwrap();
    
    let mut out = String::from("Good morning! Today's tasks:\n");
    for task in current_tasks.iter_mut() {
        if let Some(original_task_index) = all_tasks.iter().position(|r| r.label == task.label) {
            out = format!("{}\n{}\t\t🟡\t{}p\t|\t{}", out, original_task_index,task.points, task.label);
        }
    }
    let today_completed_tasks = get_today_completed_tasks();
    if !today_completed_tasks.is_empty() {
        out = format!("{}\n\nCompleted tasks: 💪🤙", out);
        for (user_id, tasks) in today_completed_tasks.iter() {
            for task in tasks.iter() {
                out = format!("{}\n\t\t\t\t🟢\t{} ({})", out, all_tasks[task.task_index as usize].label, get_name(*user_id));
            }
        }
    }
    out
}

fn get_today_completed_tasks() -> HashMap<UserId, Vec<QueTask>> {
    let completed_tasks = COMPLETED_TASKS.lock().unwrap();
    let mut map: HashMap<UserId, Vec<QueTask>> = HashMap::new();
    let today = nd_now();
    for (user_id, completed) in completed_tasks.iter() {
        let mut users_tasks_today = Vec::new();
        for task in completed.iter() {
            if today.signed_duration_since(task.date).num_seconds() == 0 {
                users_tasks_today.push(task.clone());
            }
        }
        if !users_tasks_today.is_empty() {
            map.insert(*user_id, users_tasks_today);
        }
    }
    map
}

async fn refresh_tasks() -> Result<(), Box<dyn Error + Send + Sync>> {
    info!("Refreshing tasks!");
    let today = nd_now();
    
    // claim locks
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
            if !current_tasks.iter().any(|r| r.label == all_tasks[task.task_index].label) {
                current_tasks.push(all_tasks[task.task_index].clone());
            }
     
            // schedule task to next occurance in queue
            // if somehow fails it does not modify the queue (will trigger again tommorow)
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
        Command::Claim(item_ids)     => { bot.send_message(message.chat.id, claim_task(&bot, message, item_ids)).await? },
        Command::Pass(item_id)      => { bot.send_message(message.chat.id, pass_task(&bot, message, item_id)).await? },
        Command::List                   => { bot.send_message(message.chat.id, list_tasks(&bot, message)).await? },
        Command::Score                  => { bot.send_message(message.chat.id, display_score(&bot, message)).await? },
    };
    Ok(())
}

fn claim_task(
    _: &AutoSend<Bot>,
    message: Message,
    task_indexes: String
) -> String {
    let out = {

        info!("Some user is claiming a task!");
        let all_tasks = ALL_TASKS.lock().unwrap();
        let mut current_tasks = CURRENT_TASKS.lock().unwrap();
        let mut completed_tasks = COMPLETED_TASKS.lock().unwrap();
        
        // parse tasks
        let ids: Vec<usize> = task_indexes
            .split_whitespace()
            .map(|id_str| id_str.parse::<usize>())
            .take_while(|x|x.is_ok())
            .map(|x|x.ok().unwrap())
            .collect();
        
        if ids.len() < 1 {
            return "No valid tasks claimed. Check if you claimed an avalible task. If you claimed multiple tasks, they should be split by a whitespace (/claim X Y Z).".to_string()
        }
        
        let mut out = "Use /score to see the score, or /list to see the remaining tasks.".to_string();
        let separator = if ids.len() > 1 { "\n" } else { "" };
        
        for task_index in ids.iter() {
            
            // find the claimed task
            let mut claimed_current_task_option = None;
            let mut claimed_task_index_option = None;
            for (i, task) in current_tasks.iter_mut().enumerate() {
                if task.label.eq(&all_tasks[*task_index as usize].label) {
                    claimed_current_task_option = Some(task.clone());
                    claimed_task_index_option = Some(i);
                }
            }
            
            if let Some(claimed_current_task) = claimed_current_task_option {
                info!("Found task");
                // save claimer's name for display later on the score
                set_name(&message);
                
                // insert task to user's hashmap of completed tasks
                // generate response: if user exits (if not - something wtong with message and/or teloxide)
                let response = if let Some(user) = message.from() {
                    // insert empty vector into hashmap if it's user's fisrt task
                    completed_tasks.entry(user.id).or_insert(Vec::new());
                    // insert the task into vector (as QueTask)
                    match completed_tasks.get_mut(&user.id) {
                        Some(users_competed_tasks) => {
                            users_competed_tasks.push(QueTask {
                                date: nd_now(),
                                task_index: *task_index as usize
                            });
                            format!("Claimed task {:?}! Awarded {} points! ", claimed_current_task.label, claimed_current_task.points)
                        },
                        None => format!("Something went wrong claiming task {:?}! :(", claimed_current_task.label)
                    }
                } else {
                    format!("Unable to extract user for task {:?}! :(", claimed_current_task.label)
                };
                
                // remove task from current tasks
                match claimed_task_index_option {
                    Some(index) => {current_tasks.remove(index);},
                    None => {},
                };
                
                // save new state
                // save current tasks
                match serde_any::to_file(CURRENT_TASKS_FILE_NAME, &*current_tasks) {
                    Ok(_) => {},
                    Err(e) => {error!("Error saving task queue: {:?}", e);}
                };
                // save completed tasks
                match serde_any::to_file(COMPLETED_TASKS_FILE_NAME, &*completed_tasks) {
                    Ok(_) => {},
                    Err(e) => {error!("Error saving task queue: {:?}", e);}
                };
                
                // return response
                out = format!("{}{}{}", response, separator, out);
            } else {
                out = format!("Unable to claim task (can't find task)!{}{}", separator, out);
            }
        }
        out

    };
    let rt = tokio::runtime::Runtime::new().unwrap();
    if let Err(e) = rt.block_on(refresh_tasks()) {
        return format!("Something went wrong refreshing tasks {:#?}", e.to_string());
    }
    
    out
}

fn pass_task(
    _: &AutoSend<Bot>,
    _message: Message,
    task_index: i8
) -> String {
    info!("Some user is claiming a task!");
    let all_tasks = ALL_TASKS.lock().unwrap();
    let mut current_tasks = CURRENT_TASKS.lock().unwrap();
    
    // find the claimed task
    let mut claimed_task_index_option = None;
    for (i, task) in current_tasks.iter_mut().enumerate() {
        if task.label.eq(&all_tasks[task_index as usize].label) {
            claimed_task_index_option = Some(i);
        }
    }
    match claimed_task_index_option {
        Some(index) => {
            current_tasks.remove(index);
            "Removed task from list".to_string()
        },
        None => "Unable to claim task (can't find task)!".to_string()
    }
}

fn list_tasks(
    _: &AutoSend<Bot>,
    _message: Message,
) -> String {
    construct_notification_text()
}

fn display_score(
    _: &AutoSend<Bot>,
    _message: Message,
) -> String {
    let completed_tasks = COMPLETED_TASKS.lock().unwrap();
    let all_tasks = ALL_TASKS.lock().unwrap();
    let mut out = "".to_string();
    for (user_id, tasks) in completed_tasks.iter() {
        let score = &tasks.iter().map(|t| all_tasks[t.task_index].points as i64).sum::<i64>();
        out = format!("{}\n{} -> {}", out, get_name(*user_id), score);
    }
    out    
}

fn set_name(message: &Message) -> String {
    let mut names = NAMES.lock().unwrap();
    if let Some(user) = message.from() {
        match names.get(&user.id) {
            Some(name) => name.clone(),
            None => {
                // extract data from message
                let id = user.id;
                let name = user.first_name.clone();
                // save user's name
                names.insert(id, name);
                // save changes to file
                match serde_any::to_file(NAMES_FILE_NAME, &*names) {
                    Ok(_) => {},
                    Err(e) => {error!("Error saving names: {:?}", e);}
                };
                user.first_name.clone()
            }
        }
    } else {
        "Unknown sender".to_string()
    }
}

fn get_name(user_id: UserId) -> String  {
    match NAMES.lock().unwrap().get(&user_id)  {
        Some(n) => n.clone(),
        None => "Unknown user".to_string()
    }
}

fn init_queue() {
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
) {
    let today_option = nd_now().checked_sub_signed(Duration::days(1));
    for task in all_tasks.iter() {
        if let Some(today) = today_option {
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
                        task_index,
                    })
                };
            }
        }
    }
}


fn nd_now() -> NaiveDate {
    ndt_to_nd(Utc::now().naive_utc())
}

fn ndt_to_nd(ndt: NaiveDateTime) -> NaiveDate {
    NaiveDate::from_ymd(
        ndt.year(), 
        ndt.month(), 
        ndt.day()
    )
}