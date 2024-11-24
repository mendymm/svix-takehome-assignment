use std::time::Duration;

use rand::Rng;

use crate::{types, Result};

/// For "Foo" tasks, the worker should sleep for 3 seconds, and then print "Foo {task_id}".
#[tracing::instrument(skip_all,fields(task_id=?task.id))]
pub async fn run_foo_task(task: types::Task) -> Result<()> {
    tokio::time::sleep(Duration::from_secs(3)).await;
    println!("task_id: {} | Foo {}", task.id, task.id);

    Ok(())
}
/// For "Bar" tasks, the worker should make a GET request to https://www.whattimeisitrightnow.com/ and print the response's status code
#[tracing::instrument(skip_all,fields(task_id=?task.id))]
pub async fn run_bar_task(task: types::Task) -> Result<()> {
    // todo(production): maybe reuse the connection, do we need a new tcp+tls connection for each task?
    let res = reqwest::get("https://www.whattimeisitrightnow.com/").await?;
    println!("task_id: {} | {}", res.status().as_u16(), task.id);
    Ok(())
}

/// For "Baz" tasks, the worker should generate a random number, N (0 to 343 inclusive), and print "Baz {N}"
#[tracing::instrument(skip_all,fields(task_id=?task.id))]
pub async fn run_baz_task(task: types::Task) -> Result<()> {
    let random_num = rand::thread_rng().gen_range(0..344);
    println!("task_id: {} | Baz {}", task.id, random_num);
    Ok(())
}
