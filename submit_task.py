from datetime import datetime, timedelta, UTC
import http.client
import json
import sys
import random


def random_timedelta():
    min = random.choice(range(-10, 10))
    sec = random.choice(range(-30, 30))
    return timedelta(seconds=sec, minutes=min)


def submit_random_task():
    conn = http.client.HTTPConnection("localhost:3000")
    headers = {
        "Content-Type": "application/json",
        "Accept": "application/json",
    }
    # we don't send bar task, since i don't want to dos the https://www.whattimeisitrightnow.com/ site
    task_type = random.choice(("foo", "baz"))
    execution_time = (datetime.now(UTC) - random_timedelta()).isoformat()
    json_body = {"task_type": task_type, "execution_time": execution_time}
    conn.request("POST", "/task", json.dumps(json_body), headers)
    response = conn.getresponse()
    res_text = response.read().decode("utf-8")
    status_code = response.status
    print(f"{status_code=}, {res_text=}")


def main():
    for i in range(10000):
        submit_random_task()


if __name__ == "__main__":
    main()
