# Stash config files before editing them

One simple use for `stash` is to snapshot config files before you change them.

Instead of doing this:

```bash
cp nginx.conf nginx.conf.bak
```

you can just `stash nginx.conf`

That saves the current contents of the file and records its filename
automatically. You can run the same command multiple times as you make changes
and each run will generate a new entry.

```bash
$ stash ls -l

myzkd7n0  3.3K  Apr  8 09:32  nginx.conf
fw72rf7v  3.4K  Apr  8 09:31  nginx.conf
rfpx2e48  3.4K  Apr  8 09:30  nginx.conf
```

## Compare the stashed version with the current file

After editing the file, compare the last saved snapshot with the current version:

```bash
$ diff -u <(stash cat @1) nginx.conf
--- /dev/fd/11  2026-04-08 12:35:49
+++ nginx.conf  2026-04-08 12:33:49
@@ -59,8 +59,8 @@


     server {
-        listen        one.example.com;
-        server_name   one.example.com  www.one.example.com;
+        listen        example.com;
+        server_name   example.com  www.example.com;

         access_log   /var/log/nginx.access_log  main;
```

compare the previous snapshot with the current version:

```bash
$ diff -u <(stash cat @2) nginx.conf
--- /dev/fd/11  2026-04-08 12:37:10
+++ nginx.conf  2026-04-08 12:33:49
@@ -59,8 +59,8 @@


     server {
-        listen        one.example.com;
-        server_name   one.example.com  www.one.example.com;
+        listen        example.com;
+        server_name   example.com  www.example.com;

         access_log   /var/log/nginx.access_log  main;

@@ -98,9 +98,6 @@
             root  /spool/www;
         }

-        location /old_stuff/ {
-            rewrite   ^/old_stuff/(.*)$  /new_stuff/$1  permanent;
-        }

         location /download/ {
```

compare the two last snapshots:

```bash
$ diff -u <(stash cat @2) <(stash cat @1)
--- /dev/fd/11  2026-04-08 12:38:41
+++ /dev/fd/12  2026-04-08 12:38:41
@@ -98,9 +98,6 @@
             root  /spool/www;
         }

-        location /old_stuff/ {
-            rewrite   ^/old_stuff/(.*)$  /new_stuff/$1  permanent;
-        }

         location /download/ {
```

Tag a good version in case you need it in the future.

```bash
$ stash attr @2 status=good

$ stash ls --size --date --attrs=list
myzkd7n0  3.3K  Apr  8 09:32  nginx.conf
fw72rf7v  3.4K  Apr  8 09:31  nginx.conf  good
rfpx2e48  3.4K  Apr  8 09:30  nginx.conf
```
That gives you a small history of config changes without leaving backup files
all over the working directory.

## Restore a stashed version

To restore the saved version back to the file:

```bash
stash cat @1 > nginx.conf
```

## Delete old versions

you can use `stash rm <ref>` to delete a specific entry, or `--before` to delete
all entries that are older than a specific one.

```bash
$ stash ls -l
myzkd7n0  3.3K  Apr  8 09:32  nginx.conf
fw72rf7v  3.4K  Apr  8 09:31  nginx.conf
rfpx2e48  3.4K  Apr  8 09:30  nginx.conf

$ stash rm --before myzkd7n0
Remove 2 entries older than myzkd7n0? [y/N] y

$ stash ls -l
myzkd7n0  3.3K  Apr  8 09:32  nginx.conf
```
