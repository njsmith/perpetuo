import pytest
import perpetuo
import trio
import time


def test_repeated_instantiation():
    # On x86-64, our export page is 4096 bytes, and each StallTacker has multiple
    # pointers, so >16 bytes each. So 1000 would definitely be enough to overflow, if we
    # aren't freeing them.
    for _ in range(1000):
        st = perpetuo.StallTracker("test", 1)
        st.go_idle()
        st.close()


# Doesn't really test much without the instrumentation patch, but even on vanilla python
# I guess it's good to check it doesn't crash
def test_repeated_gil():
    for _ in range(1000):
        try:
            perpetuo.instrument_gil()
        except RuntimeError:
            pass


def test_repeated_trio():
    for _ in range(1000):

        async def main():
            perpetuo.instrument_trio()
            await trio.lowlevel.checkpoint()

        trio.run(main)


# XX FIXME: figure out how to run this test in CI without breaking everywhere else,
# given the permission problems on macOS/Linux
@pytest.mark.skip
def test_catches_stall(capfd):
    assert "stall detected in process" not in capfd.readouterr()
    try:

        async def main():
            print(perpetuo.dwim())
            time.sleep(1)

        trio.run(main)

        assert "stall detected in process" in capfd.readouterr()
    finally:
        perpetuo.stop_watcher()
