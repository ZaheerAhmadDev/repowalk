"""Sample python module with two functions and a relative-import cycle."""


def run():
    """Entry shim: delegates to step_one."""
    step_one()


def step_one():
    """First half of the python cycle."""
    return _step_two()


def _step_two():
    """Second half of the python cycle."""
    return "done"


if __name__ == "__main__":
    run()
