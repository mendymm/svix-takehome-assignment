# distributed task scheduler
This project is a distributed task scheduler, it has the key feature that the user can specify an arbitrary execution time, and the tasks scheduler will ensure the task is executed *only once* when execution time <= now()

It has 2 components (that can independently scale)

1. An http service, that can exposes an API to create/list/get/delete tasks
2. An worker node, this will listen for new tasks an execute them


## How to run
NOTE: I only tested this on my machine (x86_64-unknown-linux-gnu).


You will need the following tools installed on your system

1. rust toolchain
2. [sqlx-cli](https://github.com/launchbadge/sqlx/tree/main/sqlx-cli): for applying the db migrations, and generating the .sqlx cache
3. [just](https://just.systems/man/en/): a command runner (like make)
4. docker: in order to build the container images, and run the postgres server


First build the docker image, `just build-image` this will compile the rust project in release mode.

NOTE: the dockerfile does not build the rust binary, Since building in the dockerfile will take a long time and I don't want to setup cargo chef for now

A docker-compose file is provided, it will start a postgres database, 1 http server and 10 task executer nodes. to use it run `docker compose up`

Once the service is running, use `python3 submit_task.py` to submit 10,000 tasks with execution time that varies from -30min to +30min

To make sure each task was only executed once run `docker compose logs workers | python verify_logs.py`.

## HTTP API
The API is unauthenticated, and unauthorized. Adding authentication and authorization is out of scope for this design, 
since in order to add in authn/z I will need to know who the intended use of system is. and i need to know if this system will be multi tenet

### Create a task

```
POST /task

headers:
    content-type: application/json


body: the body should be json
{
    task_type: <string> an enum of `foo`,`bar`,`baz`,
    execution_time: <string RFC3339 timestamp> the time when the task should execute. if this is in the past the worker will attempt to execute it as soon as possible
}

returns:
the uuid of scheduled task
{"task_id":"d105101b-ccac-4ca4-bc8d-f6fa34831645"}
``` 

### Delete a task
You are only able to delete a task with the `submitted` status

When the api responds with 200, then the caller is guaranteed that this task will not execute.
```
DELETE /task/<task_id>
```

### Get a task

```
GET /task/<task_id>
```

### List Tasks
```
GET /task

url params:
    status: enum of (`submitted`, `started_executing`, `done`, `failed`, `deleted`)
    type: enum of (`foo`, `bar`, `baz`)
```


# Limitations
In order to guarantee **Exactly Once** execution, while still allowing the workers to scale, and allowing the client to specify an arbitrary execution date. The system considers 3 classes of tasks

1. tasks that are submitted with task.execution_time in the far future, the worker should have a cheap way to filter these tasks out, and not load them into memory until task.execution_time <= now()+ \<max_seconds_to_sleep\>

2. tasks that are submitted with task.execution_time <= now()+ \<max_seconds_to_sleep\>, for these tasks a [tokio task](https://docs.rs/tokio/latest/tokio/task/index.html) is spawned to wait until task.execution_time <= now(), and then it will proceed to execute the task

3. tasks that are submitted task.execution_time <= now(), the system should execute these as soon as possible


### Task submitted with task.execution_time with task.execution_time >= now()+ \<max_seconds_to_sleep\>
These are tasks submitted with task.execution_time that is to far in the future for the worker nodes to simply sleep until then. for these tasks the pg_searcher thread will search the database every \<look_for_new_tasks_interval\> seconds for any tasks `where status = 'submitted' and execution_time <= (now()+ <look_for_new_tasks_interval>)` next it will submit it to the [in memory queue](#processing-tasks-in-the-in-memory-queue)


### Tasks submitted with task.execution_time with task.execution_time <= now()+ \<max_seconds_to_sleep\> (includes jobs that should run ASAP)
To communicate between the http service and the N worker nodes (since they scale independently), I use [postgres channel](https://www.postgresql.org/docs/current/sql-notify.html), this provides for a simple broadcast queue.

NOTE: since all workers will receive any notification send in the channel, make sure to set <max_concurrent_tasks_in_memory> to a high number. since some tasks will be sent to every single worker

TODO(production): do some tests on memory implications of scaling this server

Once a notification is received from the broadcast, the task is added to the [in memory queue](#processing-tasks-in-the-in-memory-queue). if the queue is full, the job is ignored: (this is very very hot code path). this will result is some tasks to wait until the next \<look_for_new_tasks_interval\> where the pg_searcher thread will add them back to the queue

Note: the pg_searcher has a very basic priority over the notification handler, this means that if the queue has capacity of 5 the pg_searcher will be able to submit a task before the notification handler

TODO(production): make sure that older tasks gets added to the in memory queue first, to avoid issue where task can wait forever

### Processing tasks in the in memory queue
This is a queue with a max size of \<max_concurrent_tasks_in_memory\>,  once in the queue the job is spawned and sleeps until execution_time <= now(). Next it acquires a [Semaphore](https://docs.rs/tokio/latest/tokio/sync/struct.Semaphore.html) with size of \<max_concurrent_executing_tasks\>. It then ensures it has an [exclusive lock](https://github.com/mendymm/svix-takehome-assignment/blob/wip/src/db.rs#L197-L238) to start executing the job


