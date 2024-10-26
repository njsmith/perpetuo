import os
import sys
import threading
import subprocess

from ._perpetuo import StallTracker


WATCHER: subprocess.Popen | None = None
GIL_STALLTRACKER: StallTracker | None = None


PR_SET_PTRACER = 0x59616D61


def linux_set_ptracer(pid):
    import ctypes

    elf_global_namespace = ctypes.CDLL(None, use_errno=True)
    res = elf_global_namespace.prctl(
        PR_SET_PTRACER, *[ctypes.c_ulong(i) for i in [pid, 0, 0, 0]]
    )
    if res < 0:
        errno = ctypes.get_errno()
        raise OSError(errno, os.strerror(errno))


def start_watcher(
    *,
    poll_interval: float | None = None,
    alert_interval: float | None = None,
    traceback_suppress: float | None = None,
    print_locals: bool = True,
    json_mode: bool = True,
) -> None:
    global WATCHER
    if WATCHER is None:
        args = []
        if poll_interval is not None:
            args += ["--poll-interval", str(poll_interval)]
        if alert_interval is not None:
            args += ["--alert-interval", str(alert_interval)]
        if traceback_suppress is not None:
            args += ["--traceback-suppress", str(alert_interval)]
        if print_locals:
            args += ["--print-locals"]
        if json_mode:
            args += ["--json-mode"]
        else:
            args += ["--no-print-locals"]
        process = subprocess.Popen(["perpetuo", *args, "watch", str(os.getpid())])
        if sys.platform == "linux":
            try:
                linux_set_ptracer(process.pid)
            except OSError:
                print()


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
        GIL_STALLTRACKER = StallTracker("GIL", "gil")
        # Conceptually, sys._set_stall_counter holds a reference to this object, so we
        # do an intentionally unbalanced incref here. In particular, this avoids the
        # StallTracker getting GC'ed when the interpreter shuts down and clears all
        # module globals, while it's actually still in use.
        GIL_STALLTRACKER._leak()
        sys._set_stall_counter(GIL_STALLTRACKER.counter_address())
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


def dwim(**kwargs) -> list[str]:
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
        start_watcher(**kwargs)
        did.append("started out of process watcher")

    return did
