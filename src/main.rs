use chrono::{NaiveDate, Utc, Datelike};
use log::info;
use teloxide::{prelude::*, utils::command::BotCommands};
use chrono::Duration;
use std::{fmt::Display, collections::HashMap};
use std::str::FromStr;
use std::error::Error;
use std::env;
use once_cell::sync::Lazy;
use std::sync::{Mutex, MutexGuard};
use serde::{de, Deserialize, Deserializer, Serialize};
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

#[derive(Debug, Clone, Deserialize)]
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
    println!("Running telegram bot!");
    teloxide::commands_repl(bot, answer, Command::ty()).await;
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
    message: Message,
    item_id: i8
) -> String {
    "TODO".to_string()
}

fn pass_task(
    _: &AutoSend<Bot>,
    message: Message,
    item_id: i8
) -> String {
    "TODO".to_string()
}

fn list_tasks(
    _: &AutoSend<Bot>,
    message: Message,
) -> String {
    "TODO".to_string()
}

fn display_score(
    _: &AutoSend<Bot>,
    message: Message,
) -> String {
    "TODO".to_string()
}

fn init_queue() -> () {
    let all_tasks = ALL_TASKS.lock().unwrap();
    let mut queue = TASK_QUEUE.lock().unwrap();
    if queue.len() == 0 {
        warn!("Initializing task que");
        fill_que(&all_tasks, &mut queue);
    } else if queue.len() != all_tasks.len() {
        warn!("Fixing queue! Missing tasks: {}", all_tasks.len() - queue.len());
    } else {
        info!("Queue is all good! :)");
    }
    match serde_any::to_file(TASK_QUEUE_FILE_NAME, &*queue) {
        Ok(_) => {},
        Err(e) => {println!("Error saving task queue: {:?}", e);}
    };
}

fn fill_que(
    all_tasks: &MutexGuard<Vec<Task>>, 
    queue: &mut MutexGuard<Vec<QueTask>>
) -> () {
    for task in all_tasks.iter() {
        let today_option = Utc::now().naive_utc().checked_sub_signed(Duration::days(1));
        if let Some(today_ndt) = today_option {
            let today = NaiveDate::from_ymd(
                today_ndt.year(), 
                today_ndt.month(), 
                today_ndt.day()
            );
            if let Some(task_date) = today.checked_add_signed(Duration::days(task.day_interval as i64)) {
                queue.push(QueTask {
                    date: task_date,
                    task_index: all_tasks.iter().position(|r| r.label == task.label).unwrap(),
                })
            }
        }
    }
}
