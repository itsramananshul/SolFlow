"""Launch an unchanged OpenPrem SDK agent against a chosen controller.

The upstream example agents hardcode their controller URL (e.g.
``Application(controller="http://localhost:8082")``). Rather than edit the
example, this shim imports the SDK, lets the agent build its Application
exactly as written, then repoints ``controller_url`` at the target controller.
The agent's name, listen port, capabilities, and crypto are untouched, so it
registers and serves exactly as it would upstream.

Usage:
    python shim.py <controller_url> <agent_file.py> [agent args...]
"""
import runpy
import sys

target = sys.argv[1].rstrip("/")
agent_file = sys.argv[2]

import openprem.agent as _agent  # noqa: E402

_orig_init = _agent.Application.__init__


def _patched_init(self, *args, **kwargs):
    _orig_init(self, *args, **kwargs)
    # Repoint at the target controller regardless of how it was passed.
    self.controller_url = target


_agent.Application.__init__ = _patched_init

# Present the agent file with its own argv (drop the shim's leading args).
sys.argv = [agent_file] + sys.argv[3:]
runpy.run_path(agent_file, run_name="__main__")
