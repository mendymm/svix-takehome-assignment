-- Add migration script here

begin;

CREATE TYPE task_type AS ENUM ('foo', 'bar', 'baz');

CREATE TYPE task_status AS ENUM ('submitted', 'started_executing', 'done', 'failed', 'deleted');

create table tasks(
    "id" UUID constraint tasks_pk primary key,
    "created_at" timestamptz default current_timestamp not null,
    "status" task_status not null,
    "execution_time" timestamptz not null,
    "task_type" task_type not null,    
    "started_executing_at" timestamptz,
    "completed_at" timestamptz,
    "failed_at" timestamptz,
    "deleted_at" timestamptz
);

create index tasks_execution_time on tasks ("execution_time");
create index tasks_status on tasks ("status");
create index tasks_task_type on tasks ("task_type");

commit;