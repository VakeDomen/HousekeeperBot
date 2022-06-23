use teloxide::{prelude::*, utils::command::BotCommands};
use std::collections::HashMap;
use std::error::Error;
use std::env;
use once_cell::sync::Lazy;
use std::sync::Mutex;
use dotenv::dotenv;

const CURRENT_TASK_FILE_NAME: &str = "current_tasks.json";

static CURRENT_TASKS: Lazy<Mutex<HashMap<ChatId, Vec<String>>>> = Lazy::new(|| {
    match serde_any::from_file(CURRENT_TASK_FILE_NAME.to_string()) {
        Ok(hm) => Mutex::new(hm),
        Err(_) => Mutex::new(HashMap::new())
    }
});

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