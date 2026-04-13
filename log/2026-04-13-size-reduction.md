# I started the day with a 1.5M binary, ended with 800K.

Date: 2026-04-13

I spent some time looking at the release binary size and at a few hot paths
that were doing more allocation or copying than necessary.

One important change before the actual size-tuning commits was splitting shell
completion generation out of the main binary. Clap makes it easy to have
a `completion` subcommand, but this is something most users will run once,
during installation, and then load to memory every time they run `stash`.

I removed the completion subcommand and moved completion-related code into
a separate `stash-completion` binary, which reduced the main binary size
by about 300K.

Another direct size win came from disabling unused Clap features
and setting `panic = "abort"` in the release profile. That removed some baggage
from the final binary and cut it by about 200KB, from roughly 1.1MB to 911KB.

I also did some work on small memory and speed optimizations.
Nothing flashy, mostly about removing small, unnecessary costs
and adding benchmarks so I could see whether a change actually helped.

Most notable:

- `1067a13` removed avoidable overhead in common paths. I changed several
  small things there. IDs stopped allocating just to lowercase data that was
  already lowercase. Display escaping and padding now borrow in the common
  case instead of allocating new strings. Date formatting computes "now" once
  per listing instead of once per row. I also added full-listing benchmarks so
  I could measure `ls -l` and `ls --json` over the whole stash instead of only
  the limited `-n 20` case.

- `3ab5526` did the same for `cat`. The push path was already
  using a 64KB buffer, but `cat` was still reading through a smaller default
  setup. I changed `cat_to_writer` to use a 64KB `BufReader`, mostly for
  better throughput on larger entries. The benchmark on a 10MB entry improved
  by about 15%.

Overall: 40% binary size reduction, and small performance inprovements.
