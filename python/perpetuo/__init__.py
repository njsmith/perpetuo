__all__ = ["StallTracker"]

from ._perpetuo import StallTracker

import sys as _sys

if hasattr(_sys, "_set_stall_counter"):
    gil_tracker = StallTracker("GIL", 1)
    _sys._set_stall_counter(gil_tracker.counter_address())
