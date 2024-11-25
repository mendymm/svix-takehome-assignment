import sys

# make sure each task was executed once
# read logs from docker compose logs workers | python verify_logs.py
all_task_ids = set()
for line in sys.stdin.readlines():
    # workers-5   | task_id: 907be2f3-c633-444a-ae6d-0ccdd876e50a | Foo 907be2f3-c633-444a-ae6d-0ccdd876e50a
    log = "|".join(line.split("|")[1:]).strip()
    # task_id: 907be2f3-c633-444a-ae6d-0ccdd876e50a | Foo 907be2f3-c633-444a-ae6d-0ccdd876e50a
    if log.startswith("task_id"):
        # sqlx might warn about commit taking too long
        task_id = log.split("task_id: ")[1].split("|")[0].strip()
        assert task_id not in all_task_ids, task_id
        all_task_ids.add(task_id)
