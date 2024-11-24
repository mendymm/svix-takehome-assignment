

db_user := "postgres"
db_password := "password"
db_name := "svix_tasks"
db_port := "5432"

default:
  @just --list

start-dev-db:
  #!/usr/bin/bash
  set -eux
  # -N 1000 (raise max connections from 100 to 1000)
  docker run \
    --name svix_dev_db \
    -e POSTGRES_USER={{db_user}} \
    -e POSTGRES_PASSWORD={{db_password}} \
    -e POSTGRES_DB={{db_name}} \
    -p {{db_port}}:5432 \
    -d postgres:17 \
    postgres -N 1000

  export PGPASSWORD="{{db_password}}"
  until psql -h "localhost" -U "{{db_user}}" -p "{{db_port}}" -d "postgres" -c '\q'; do
      echo >&2 "Postgres is still unavailable - sleeping"
      sleep 1
  done
  echo >&2 "Postgres is up and running on port {{db_port}}!"


  export DATABASE_URL="postgres://{{db_user}}:{{db_password}}@localhost:{{db_port}}/{{db_name}}"
  sqlx database create
  sqlx migrate run
  echo >&2 "Postgres has been migrated, ready to go!"


[no-cd]
rust-pre-commit:
  #!/usr/bin/env bash
  set -euxo pipefail
  cargo fmt
  cargo clippy --all-targets -- -D warnings
  cargo sqlx prepare
  cargo test

build-release: rust-pre-commit
  SQLX_OFFLINE=true cargo build --release
  

spawn-n-workers N_WORKERS: build-release
  #!/usr/bin/env bash
  set -euxo pipefail
  echo {{N_WORKERS}}
  for i in $(seq 1 {{N_WORKERS}}); do 
    RUST_LOG="error" ./target/release/svix-takehome-assignment executor > logs/worker_$i.log &
  done

build-image: build-release
   docker image build --tag svix-takehome-assignment .


