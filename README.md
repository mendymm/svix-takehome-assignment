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
