from functools import partial
import os
import sys
import threading
import time

import trio
import perpetuo


async def naughty():
    while True:
        await trio.sleep(1)
        time.sleep(1)


async def foo():
    await naughty()


def gil_naughty():
    test_local = "hello"
    while True:
        time.sleep(1)
        perpetuo.stall_gil(1)


async def main():
    print(perpetuo.dwim())

    some_local = {"a": 1}
    another_local = 3
    print(f"pid {os.getpid()}")
    thread = threading.current_thread()
    print(f"{thread.ident=} {thread.native_id=}")
    async with trio.open_nursery() as nursery:
        nursery.start_soon(
            partial(trio.to_thread.run_sync, gil_naughty, cancellable=True)
        )
        await foo()


trio.run(main)
