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
stash ls --meta source=usgs-earthquakes
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

### 5. Keep the stream flowing while also saving it

If you want to inspect the data immediately while stashing it, use `stash tee`:

```bash
curl -s \
  'https://earthquake.usgs.gov/earthquakes/feed/v1.0/summary/2.5_month.geojson' \
  | stash tee -m source=usgs-earthquakes -m stage=raw \
  | jq '.metadata'
```

`stash tee` writes the original bytes to stdout and also stores them as a new
stash entry. The new ID is printed to stderr, so it does not interfere with the
pipeline.
