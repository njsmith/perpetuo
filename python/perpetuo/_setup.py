import os
import sys
import threading
import subprocess

from ._perpetuo import StallTracker


WATCHER: subprocess.Popen | None = None
GIL_STALLTRACKER: StallTracker | None = None


def start_watcher(*, poll_interval: float | None = None) -> None:
    global WATCHER
    if WATCHER is None:
        if poll_interval is not None:
            poll_args = ["--poll-interval", str(poll_interval)]
        else:
            poll_args = []
        subprocess.Popen(["perpetuo", "watch", str(os.getpid())] + poll_args)


def stop_watcher() -> None:
    global WATCHER
    if WATCHER is not None:
        WATCHER.kill()
        WATCHER.wait()
        WATCHER = None


def instrument_gil() -> None:
    global GIL_STALLTRACKER
    if GIL_STALLTRACKER is not None:
        return
    if hasattr(sys, "_set_stall_counter"):
        GILL_STALLTRACKER = StallTracker("GIL", "gil")
        sys._set_stall_counter(GILL_STALLTRACKER.counter_address())
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

        def _init(self):
            assert self.stall_tracker is None
            thread = threading.current_thread().ident
            self.stall_tracker = StallTracker(
                f"Trio run loop (thread {thread:#_x})", thread
            )

        def before_task_step(self, _: trio.lowlevel.Task) -> None:
            if self.stall_tracker is None:
                self._init()
                # New StallTracker starts out in the "active" state, which is what we
                # want
            else:
                self.stall_tracker.go_active()

        def after_task_step(self, _: trio.lowlevel.Task) -> None:
            if self.stall_tracker is None:
                self._init()
                assert self.stall_tracker is not None
                # New StallTracker starts out in the "active" state, so need to toggle
                # it
            self.stall_tracker.go_idle()

        def after_run(self) -> None:
            self.stall_tracker.close()

    trio.lowlevel.add_instrument(TrioStallInstrument())


def dwim(*, poll_interval: float | None = None) -> list[str]:
    did = []

    try:
        instrument_gil()
    except RuntimeError:
        pass
    else:
        did.append("instrumented GIL")

    if "trio" in sys.modules:
        try:
            instrument_trio()
        except RuntimeError:
            pass
        else:
            did.append("instrumented Trio")

    if did:
        start_watcher(poll_interval=poll_interval)
        did.append("started out of process watcher")

    return did
