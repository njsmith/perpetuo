from functools import partial
import os
import sys
import threading
import time

import trio
from perpetuo import StallTracker
from perpetuo._perpetuo import _stall_gil


class TrioStallInstrument(trio.abc.Instrument):
    def __init__(self):
        # We construct the tracker lazily, to make sure that we're already in Trio
        # before it becomes active
        self.stall_tracker: StallTracker | None = None

    def before_task_step(self, _: trio.lowlevel.Task) -> None:
        if self.stall_tracker is None:
            self.stall_tracker = StallTracker(
                "Trio run loop", threading.current_thread().ident
            )
            # New StallTracker starts out in the "active" state
        else:
            self.stall_tracker.go_active()

    def after_task_step(self, _: trio.lowlevel.Task) -> None:
        if self.stall_tracker is None:
            self.stall_tracker = StallTracker(
                "Trio run loop", threading.current_thread().ident
            )
            # New StallTracker starts out in the "active" state
        self.stall_tracker.go_idle()


async def naughty():
    while True:
        await trio.sleep(1)
        time.sleep(1)


async def foo():
    await naughty()


def gil_naughty():
    while True:
        time.sleep(1)
        _stall_gil(1)


async def main():
    trio.lowlevel.add_instrument(TrioStallInstrument())
    print(f"pid {os.getpid()}")
    thread = threading.current_thread()
    print(f"{thread.ident=} {thread.native_id=}")
    async with trio.open_nursery() as nursery:
        nursery.start_soon(
            partial(trio.to_thread.run_sync, gil_naughty, cancellable=True)
        )
        await foo()


trio.run(main)
