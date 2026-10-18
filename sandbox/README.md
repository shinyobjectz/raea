# sandbox

A WASI sandbox for the agent's command-line tools, built to run on the iPhone, which allows no JIT.

- `wsx/`: runs WASI modules compiled ahead of time for Pulley on Wasmtime's interpreter, with only the project folder
  visible, a fuel limit and a memory cap, and logs every call as an action row. `wsxc` compiles modules; `wsx-suite`
  runs the test suite.
- `rtools/`: the everyday file and text commands (`ls`, `cat`, `grep`, `sed`, `find`, ...) in one small WASI module.
- `probe/`: an iOS app that measures the sandbox's size and checks it on a device.
- `build.sh`: builds the tools and modules, and with `phone`, the iPhone library and probe apps.

[`react/`](../react) links `wsx` behind an optional feature. The agent's shell ([`shell/`](../shell)) runs WASI tools
through it only for commands it doesn't provide itself.

Status: experimental.
