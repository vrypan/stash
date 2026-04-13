# Implementing "append" and changes in "-a"

Date: 2026-04-13

Using immutable entries identified by ULIDs simplifies many things. For example,
you can be relatively confident that multiple commands running in parallel
will not overwrite the same file. That means no lock files, which keeps the
code simpler and faster. Simple and fast are important properties for stash.
I have actively removed features that came with a complexity, speed, or memory
penalty.

That said, being able to append data to an entry is something that is handy.

I ended up with a different approach: each entry stays immutable, nothing
changes there. But `stash cat` should be able to cat multiple entries that
share the same attribute, or the same attribute value.

If this works, instead of "appending to an entry", I can "append to a key",
and do something like

```
echo "hello " | stash -a example=1
echo "world!" | stash -a example=1

stash cat -a example=1

hello world!
```

But this exposed an existing inconsistency in how `-a` is used:

- in `stash ls`, `-a key` meant _"show a column with `key` values"_
- in `stash rm` and now `stash cat`, it means _"filter entries where key is set"_

**This is not good**. CLI arguments are a mini-language that should feel intuitive and consistent.

So I had to make a small, but **breaking**, change: After `v0.8.0`, `-a key` always
means _"filter by key"_.

Now, if you're familiar with `stash ls -a`, it is obvious what `stash cat -a` will do.
And making `-a +key` mean _"add a column to show the key value"_ and `-a ++key` mean
_"show entries where key is set, and show the key value as a column"_ makes sense.

|command|-a|meaning|
| -- | -- | -- |
|push, tee|key=value|set key=value|
|ls, cat, rm|key|filter entries where key is set|
|ls, cat, rm|key=value|filter entries where key=value|
|ls|+key|show a separate key column|
|ls|++key|filter by key and show a key column|
