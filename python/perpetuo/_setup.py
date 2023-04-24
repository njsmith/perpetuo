import sys
import threading
import subprocess
import os

from ._perpetuo import StallTracker


WATCHER = None


def start_watcher() -> None:
    global WATCHER
    if WATCHER is None:
        subprocess.Popen(["perpetuo", "watch", str(os.getpid())])


def instrument_gil() -> None:
    if hasattr(sys, "_set_stall_counter"):
        gil_tracker = StallTracker("GIL", "gil")
        sys._set_stall_counter(gil_tracker.counter_address())
    else:
        raise RuntimeError(
            "This Python was not built with the perpetuo GIL instrumentation patch"
        )


def instrument_trio() -> None:
    import trio

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

    trio.lowlevel.add_instrument(TrioStallInstrument())


def dwim() -> None:
    instrumented_something = False

    try:
        instrument_gil()
    except RuntimeError:
        pass
    else:
        instrumented_something = True

    if "trio" in sys.modules:
        try:
            instrument_trio()
        except RuntimeError:
            pass
        else:
            instrumented_something = True

    if instrumented_something:
        start_watcher()
