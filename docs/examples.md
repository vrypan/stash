# Examples

## Cache a heavy JSON fetch and build multiple views

This example uses the public USGS earthquake GeoJSON feed for magnitude 2.5+
earthquakes in the past 30 days:

```text
https://earthquake.usgs.gov/earthquakes/feed/v1.0/summary/2.5_month.geojson
```

It shows how to:
- fetch a large JSON payload once
- stash the raw response
- transform it with `jq`
- stash the reduced dataset
- create two different views from the stashed data without re-running `curl`

### 1. Fetch the raw feed and stash it

```bash
curl -s \
  'https://earthquake.usgs.gov/earthquakes/feed/v1.0/summary/2.5_month.geojson' \
  | stash -m source=usgs-earthquakes -m stage=raw
```

This creates a stash entry containing the full GeoJSON response.

If you want to confirm that it was saved:

```bash
stash ls -l --meta source=usgs-earthquakes
```

### 2. Transform the raw feed and stash the reduced dataset

```bash
stash cat @1 \
  | jq '
      {
        generated: .metadata.generated,
        count: .metadata.count,
        earthquakes: [
          .features[]
          | {
              time: .properties.time,
              magnitude: .properties.mag,
              place: .properties.place,
              tsunami: .properties.tsunami,
              url: .properties.url
            }
        ]
      }
    ' \
  | stash -m source=usgs-earthquakes -m stage=reduced
```

Now you have two related entries:
- `@2` is the raw USGS feed
- `@1` is the reduced dataset

The expensive network fetch happened only once.

### 3. Create a table view from the reduced dataset

```bash
stash cat @1 \
  | jq -r '
      .earthquakes[]
      | [
          (.magnitude | tostring),
          (.time / 1000 | strftime("%Y-%m-%d %H:%M:%S")),
          .tsunami,
          .place
        ]
      | @tsv
    '
```

This produces a TSV table with:
- magnitude
- UTC time
- tsunami flag
- place

If you want headers too:

```bash
{
  printf 'mag\ttime_utc\ttsunami\tplace\n'
  stash cat @1 \
    | jq -r '
        .earthquakes[]
        | [
            (.magnitude | tostring),
            (.time / 1000 | strftime("%Y-%m-%d %H:%M:%S")),
            .tsunami,
            .place
          ]
        | @tsv
      '
} | column -t -s $'\t'
```

### 4. Create an Alaska-only table from the same reduced dataset

```bash
{
  printf 'mag\ttime_utc\ttsunami\tplace\n'
  stash cat @1 \
    | jq -r '
        .earthquakes[]
        | select(.place | test("Alaska"; "i"))
        | [
            (.magnitude | tostring),
            (.time / 1000 | strftime("%Y-%m-%d %H:%M:%S")),
            .tsunami,
            .place
          ]
        | @tsv
      '
} | column -t -s $'\t'
```

This gives you a second view, filtered to earthquakes whose `place` mentions
Alaska, without touching the network again.

### 5. After a few minutes, check whether new earthquakes were registered

Fetch the feed again and stash a new raw snapshot:

```bash
curl -s \
  'https://earthquake.usgs.gov/earthquakes/feed/v1.0/summary/2.5_month.geojson' \
  | stash -m source=usgs-earthquakes -m stage=raw
```

Transform that new raw snapshot into the same reduced shape and stash it too:

```bash
stash cat @1 \
  | jq '
      {
        generated: .metadata.generated,
        count: .metadata.count,
        earthquakes: [
          .features[]
          | {
              time: .properties.time,
              magnitude: .properties.mag,
              place: .properties.place,
              tsunami: .properties.tsunami,
              url: .properties.url
            }
        ]
      }
    ' \
  | stash -m source=usgs-earthquakes -m stage=reduced
```

At this point:
- `@1` is the new reduced snapshot
- `@2` is the new raw snapshot
- `@3` is the previous reduced snapshot
- `@4` is the previous raw snapshot

If you have stashed other data in the meantime, use `stash ls -l` to find the ids of the entries
you are interested in, and use them in place of `@1`, `@3`.

```bash
# example
stash ls -l --meta source --meta stage

g5xa4znm  412.0K  Tue Apr  1 13:35:40 2026 +0300  01kn4z3q4vv5crxjdkg5xa4znm  [usgs-earthquakes  reduced]
4p0rgpda    1.3M  Tue Apr  1 13:35:00 2026 +0300  01kn4z358zf1fme79d4p0rgpda  [usgs-earthquakes  raw]
w6sz0cbw  411.3K  Tue Apr  1 13:12:10 2026 +0300  01kn4y4m147f06r4few6sz0cbw  [usgs-earthquakes  reduced]
6ya0x77f    1.3M  Tue Apr  1 13:11:21 2026 +0300  01kn4y3xjtj40kksg16ya0x77f  [usgs-earthquakes  raw]
```

Now compare the two reduced snapshots and show only earthquakes that are new in
the latest fetch:

```bash
{
  printf 'mag\ttime_utc\ttsunami\tplace\n'
  jq -nr \
    --slurpfile new <(stash cat @1) \
    --slurpfile old <(stash cat @3) '
      ($old[0].earthquakes | map(.url) | INDEX(.)) as $seen
      | $new[0].earthquakes
      | map(select($seen[.url] | not))
      | .[]
      | [
          (.magnitude | tostring),
          (.time / 1000 | strftime("%Y-%m-%d %H:%M:%S")),
          .tsunami,
          .place
        ]
      | @tsv
    '
} | column -t -s $'\t'
```

This works because the reduced snapshots keep the USGS event `url`, which is a
stable identifier for each earthquake record.
