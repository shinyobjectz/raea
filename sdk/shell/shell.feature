@spec
Feature: @raea/shell, the agent's shell
  just-bash (vercel-labs, Apache-2.0, pinned at 3.6.0 and used unmodified as a dependency) runs the bash: parsing,
  pipes, redirections, functions, loops and its ~80 text tools, in TypeScript on JavaScriptCore (raea-native's runtime, the
  same on the Mac and the iPhone). Under it the filesystem is the project folder through raea-native, and raea's own
  tools are commands that call the engine natively. The shell is the agent's hands in a space.

  Rule: The project folder is the filesystem, and nothing else
    Scenario: Paths
      Given a Shell opened on a project folder ({root})
      Then "/" in the shell is the project's root, every path is a file under it, and ".." can't climb above it
      And a symlink can't lead out: following one that resolves outside the folder fails ("permission denied
        (outside the project)"), and an absolute link target is a project path (it reads back as written)
      And a link the host placed (fs.place_link: a space's media, a project's takes) may lead outside the folder:
        what's behind it reads (cat, ls, stat, cp from it, rdb opening it), never writes (a write, an append, an rm
        through it fails), and no outside name shows (readlink says "(outside the project)", readlink -f gives the
        shell's path); the shell itself can't make such a link: a symlink whose target climbs out, or a hard link to
        a file outside, is refused
      And a path with a null byte is refused by name, and the records folder (.raea/) isn't visible or writable
      And an error the interpreter doesn't report itself fails the command (exit 1, the message on stderr), never the
        caller; a write refused behind a redirect is one (just-bash writes a redirect's target after the command,
        outside any catch), so for now it ends the whole command line rather than only its own command as in bash
      And just-bash's IFileSystem contract (its interface.contract.test.ts, run on RaeaFs) passes in full
    Scenario: Each command is one action row
      When the shell runs a command line
      Then .raea/actions.jsonl gets one row: seq, the command, its verb and arguments, the files it read, each file it
        wrote with its state before and after (a file's content hash and mode, a folder, a link, or absent), bytes,
        exit, time, the error's first line, and the project's state hash before and after (a chain over the rows; a
        command that changed nothing leaves it as it was)
      And contents are kept by hash in .raea/objects/, so `undo()` puts the last command's paths back as they were and
        `redo()` makes them again (each is a row of its own); a reopened project keeps its history
      And a tool that writes outside RaeaFs (rdb, raea-motion, sqlite3, a WASI tool) has every path argument
        kept first (fs.touch), so its row and its undo cover what it wrote

  Rule: raea's tools are commands, run natively in process
    Scenario: The engine's command lines
      Then `rdb` and `raea-motion` run their own command lines in this process (no subprocess, so the same on
        the iPhone) in the shell's folder, with their stdout and stderr captured; an absolute or climbing path argument
        is the project's file, and the project's root never shows in what they print
    Scenario: The engine's libraries as small command lines
      Then `solid EXPR [volume|bounds|triangles|empty|slice Z]` builds a Manifold solid from rdb's shape expression
      And `ecs WORLD.json spawn|set|despawn|query|count|rows|column ...` keeps a world's rows in a project file and
        runs each step on the engine's tables
      And `media transcode IN OUT [--from --to --height --kbps]` is the native trim, scale and encode with sound; `media
        size FILE`; `rdb media ...` has the rest of the media verbs (and `rdb ffmpeg`, `rdb ffprobe`)

  Rule: The heavy extras are native
    Scenario: sqlite3
      Then `sqlite3 [options] [DB] [SQL]` is SQLite itself (bundled, natively) on a project file, or :memory:, with SQL
        from the argument or stdin; its modes are -list (default, "|"), -csv, -json, -line, -column, -table, -box,
        -markdown, -tabs, -quote, -html, -ascii, with -header, -separator, -newline, -nullvalue, -readonly, -echo, -cmd
      And an error ends the script with exit 1 and "Error: ..." on stderr
      And (rdb's own SQL is for edits: `rdb query EDIT.rdb`; sqlite3 is for the databases people bring)
    Scenario: js-exec
      Then `js-exec -c CODE | FILE [ARGS]` runs JavaScript or TypeScript as an ES module in a fresh JavaScriptCore context on its own
        thread (transpiled in process by raea-modules, deno_ast: no bun, so the same on the iPhone), with console to
        stdout and stderr, process.argv, env, exit and stdin.text, and timers
      And imports resolve only to files in the project; native calls are limited to the web shims' (no filesystem, no
        network: a call outside the list throws "not allowed here"); a run past --timeout MS (30000) ends with exit 124
    Scenario: curl
      Then curl exists only when the host gives the Shell a network policy, and goes through raea-native's net.fetch:
        a request is sent only when its URL's origin and path prefix and its method are allowed (GET and HEAD unless
        listed), and every redirect hop is checked again
      And the policy's headers are added at the boundary, a secret named by the host's environment variable
        (secretHeaders), so its value never enters JavaScript and a command can't override it
    Scenario: python
      Then there is no python (raea ships none): `python3` is "command not found", exit 127

  Rule: One terminal per space, for hosts outside JavaScript
    Scenario: Kept between commands
      Then a Shell is one terminal: a `cd` or an `export` carries over to the next command (just-bash resets them per
        exec; the Shell passes the last folder and exported variables back in), and /dev/null, /dev/stdin, /dev/stdout,
        /dev/stderr are devices, not project files
    Scenario: The server and the CLI (void's agent, in Rust)
      Then `raea-motion shell-serve ROOT [--network POLICY.json]` keeps one Shell and answers JSON lines: a request
        {id, op: exec|undo|redo|log, cmd?, timeout_ms?, last?} gets one reply {id, stdout, stderr, exitCode, row}
        (undo and redo: row; log: rows; a bad request: error); the first line says {ready, root}
      And a command past timeout_ms stops at its next statement with exit 124 and the shell goes on; stdout and stderr
        are cut at 64 KB each, saying how much was left out
      And `raea-motion shell ROOT -c CMD [--timeout MS] [--network POLICY.json]` answers one request the same way

  Rule: just-bash and wsx split the work
    Scenario: Who runs what
      Then just-bash is the shell and its text tools (ls, cat, grep, sed, awk, jq, sort, find, ...): one implementation
        of each, in TypeScript on JavaScriptCore, on RaeaFs
      And wsx keeps anything compiled to WASI (Wasmtime, Pulley on the iPhone): each of its tools the shell lacks (jaq)
        is a command run with only the project folder visible, under its fuel and memory limits; a WASI tool never
        shadows a just-bash or raea command of the same name

  Rule: The same answers on the Mac and the iPhone
    Scenario: The probe command set
      Then sdk/shell/conformance/probe.test.ts runs the sandbox probe's command set (ls, wc, sort, head, grep, jq, a write
        and read back, both escapes, find, sed) and the rest of the tools through the shell on a fixed project, and
        every command's stdout and exit equal probe-golden.json (recorded on the Mac) on both machines
        (sdk/react/ios/test.sh runs it on the iPhone)
    Scenario: just-bash's own suite
      Then sdk/shell/tools/upstream.sh runs just-bash's tests at the pinned tag on raea's JavaScriptCore runtime, bundled as upstream bundles its
        browser build; the parts that need Node by design (python3, sqlite3 and js-exec's workers, the real-disk
        filesystems, the CLI, curl's mocked transport) are listed, not run
