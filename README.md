# Perpetuo

> *perpetuo*, verb: To cause to continue uninterruptedly, to proceed with
> continually

Perpetuo is a stall tracker for Python. Specifically, it can detect when:

- One thread is holding the GIL for too long, blocking other threads from having
  a chance to run (requires an instrumented version of CPython)

- One Trio task is running too long without checkpointing, blocking other tasks
  from having a chance to run

The actual monitoring is done from a separate process, using a customized
version of [py-spy](https://github.com/benfred/py-spy). So the monitoring is
very low overhead and should not interfere with the monitored process at all.
The goal is to be able to use this in production.


## Quickstart

1. `pip install perpetuo`
2. Optional: patch CPython (see below)
3. `import perpetuo`
4. If you're using Trio: call `perpetuo.dwim()` inside `trio.run`

   If you're not using Trio: call `perpetuo.dwim()` anywhere 
5. Optional: log `perpetuo.dwim()`'s return value to see what it did


## Available API

`perpetuo.start_watcher()`: Spawns the monitoring process in the background. 

`perpetuo.instrument_gil()`: Enables GIL instrumentation, or raises
`RuntimeError` if you don't have the patched version of CPython.

`perpetuo.instrument_trio()`: Enables Trio instrumentation. Must be called
inside `trio.run`.

`perpetuo.dwim()`: Attempts to call all the above functions as appropriate, and
returns a list of strings describing which operations it actually performed. If
you're using Trio, make sure to call it inside `trio.run`.

`StallTracker`: Low-level class that allows you to add custom instrumentation to
other things. See source for details.


## Patching CPython to instrument the GIL

There are patches available for:

CPython version 3.10.*:

  https://github.com/python/cpython/compare/3.10...njsmith:cpython:njs/perpetuo-gil.diff

CPython version 3.11:

  https://github.com/python/cpython/compare/3.11...njsmith:cpython:njs/perpetuo-gil-3.11.diff

CPython version 3.12:

  https://github.com/python/cpython/compare/3.12...njsmith:cpython:njs/perpetuo-gil-3.12.diff

## Bonus

[Niccolò Paganini's *Moto Perpetuo*, performed by Antal Zalai](https://www.youtube.com/watch?v=D-TAO7U6rtg)
