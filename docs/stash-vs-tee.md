# Why `make 2>&1 | stash tee` is better than `make 2>&1 | tee build.log`

You've probably seen this pattern more than once:

``` bash
make 2>&1 | tee build.log
```

It works, but it forces you to decide on a filename, manage log files,
and remember where output was written. 

If you have created `build2.log`, `build3.log`, `build-nodeps3.log`, and so on,
raise your hand. And maybe more than once, you overwrote one of these logs only to
realize that you needed it later. And maybe you have come back to a long-running
built, to realize you're not sure which log corresponds to the configuration that worked.

-   You must invent a filename every time
-   Logs pile up in working directories
-   You overwrite previous runs unless you rename files
-   You lose the natural ordering of runs
-   Cleanup becomes manual

A stash-based workflow removes that friction while preserving everything 
you expect from `tee`.

``` bash
make 2>&1 | stash tee
```

What happens in this case:

-   Output is shown live (like `tee`)
-   Output is stored automatically
-   Each run becomes a new entry
-   Entries are ordered by time
-   Retrieval is trivial

```bash
# You can list your stash entries:
stash ls -l

ak4x9sr1  99B  Thu Apr  2 22:03:51 2026 +0300  01kn7s9624z4dpxz7nak4x9sr1
n5pwa78h  99B  Thu Apr  2 22:03:41 2026 +0300  01kn7s8wd35es5txscn5pwa78h
90e66f4b  99B  Thu Apr  2 22:01:09 2026 +0300  01kn7s47q5t9k8kfzb90e66f4b
aczsve56  99B  Thu Apr  2 21:58:35 2026 +0300  01kn7rzhew4tghsh4aaczsve56

# You can cat them by id
stash cat ak4x9sr1

# or by recency
stash cat @1
```

No filenames. No collisions. No cleanup.

If you want, you can attach arbitrary metadata:

```
stash attr @1 cpp=gcc-15.2.0

# the last column is the attribute you just set
stash ls -l --attr +cpp
ak4x9sr1  99B  Thu Apr  2 22:03:51 2026 +0300  01kn7s9624z4dpxz7nak4x9sr1  gcc-15.2.0
n5pwa78h  99B  Thu Apr  2 22:03:41 2026 +0300  01kn7s8wd35es5txscn5pwa78h  gcc-15.1.0
90e66f4b  99B  Thu Apr  2 22:01:09 2026 +0300  01kn7s47q5t9k8kfzb90e66f4b  gcc-15.1.0
aczsve56  99B  Thu Apr  2 21:58:35 2026 +0300  01kn7rzhew4tghsh4aaczsve56  gcc-15.2.0

stash attr @1 note="use-blake3"

stash ls -l -a +cpp -a +note
ak4x9sr1  99B  Thu Apr  2 22:03:51 2026 +0300  01kn7s9624z4dpxz7nak4x9sr1  gcc-15.2.0  use-blake3
n5pwa78h  99B  Thu Apr  2 22:03:41 2026 +0300  01kn7s8wd35es5txscn5pwa78h  gcc-15.1.0
90e66f4b  99B  Thu Apr  2 22:01:09 2026 +0300  01kn7s47q5t9k8kfzb90e66f4b  gcc-15.1.0
aczsve56  99B  Thu Apr  2 21:58:35 2026 +0300  01kn7rzhew4tghsh4aaczsve56  gcc-15.2.0

# if you create entries from various jobs, you can set an attribute when the entry is created
cargo build --release 2>&1 | stash tee -a type=build.log
cargo test 2>&1 | stash tee -a type=tests.log

# deleting old entries is easy
stash rm --before n5pwa78h
Remove 2 entries older than n5pwa78h? [y/N] y

stash ls -l --attr +cpp --attr +note
ak4x9sr1  99B  Thu Apr  2 22:03:51 2026 +0300  01kn7s9624z4dpxz7nak4x9sr1  gcc-15.2.0  use-blake3
n5pwa78h  99B  Thu Apr  2 22:03:41 2026 +0300  01kn7s8wd35es5txscn5pwa78h  gcc-15.1.0
```

- No filename decisions: Run the same command again and again, without worrying you may overwrite the previous log.
- Natural stack semantics: stash entries are lexicographically ordered
- No workspace file pollution: Your working directory remains clean.

**`tee` writes to a file.**   
**`stash tee` writes to history.**

## Return Codes

`stash tee` is intentionally close to `tee` as a pipeline component, but it can
also preserve captured output in the stash when something interrupts the
stream.

| Situation | `tee` | `stash tee` |
| --- | --- | --- |
| Normal completion | `0` | `0` |
| Downstream closes early (`head`, `column`, etc.) | typically `0` in common pipeline use | `0` |
| `Ctrl-C` / `SIGINT` | `130` | `130` |
| `SIGTERM` | `143` | `143` |
| Non-signal I/O or processing error | usually non-zero | `1` |

### `--save-on-error`

`stash tee` accepts `--save-on-error=true|false` and defaults to `true`.

When `--save-on-error=true`:
- `SIGINT` / `Ctrl-C`: if bytes were already captured, `stash tee` saves a
  partial entry, marks it with `partial=true`, and exits `130`
- `SIGTERM`: if bytes were already captured, `stash tee` saves a partial
  entry, marks it with `partial=true`, and exits `143`
- non-signal read or processing error: if bytes were already captured,
  `stash tee` saves a partial entry, marks it with `partial=true`, and exits
  `1`

When `--save-on-error=false`:
- `SIGINT`, `SIGTERM`, or non-signal read/processing errors do not keep a
  partial entry

Broken pipe is separate from `--save-on-error`:
- if the downstream consumer closes early (`head`, `column`, etc.), `stash tee`
  still exits `0`
- if any input was already captured, the saved entry is kept
