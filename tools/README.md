# tools

Build helpers shared by every crate.

- `link-jit.sh`: the linker wrapper `.cargo/config.toml` sets for `aarch64-apple-darwin`. It links normally, then
  signs the binary with `jit.entitlements`, because JavaScriptCore keeps its JIT off in a binary without the JIT
  entitlement.
- `jit.entitlements`: that entitlement.
