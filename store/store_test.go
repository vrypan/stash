package store

import (
	crand "crypto/rand"
	"encoding/json"
	"errors"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/oklog/ulid/v2"
)

func setupTempStash(t *testing.T) string {
	t.Helper()
	root := filepath.Join(t.TempDir(), "stash")
	t.Setenv("STASH_DIR", root)
	if err := Init(); err != nil {
		t.Fatalf("Init: %v", err)
	}
	return root
}

func randomBytes(t *testing.T, n int) []byte {
	t.Helper()
	buf := make([]byte, n)
	if _, err := crand.Read(buf); err != nil {
		t.Fatalf("random bytes: %v", err)
	}
	return buf
}

func createImportedEntry(t *testing.T, root string, ts time.Time, data []byte, attrs map[string]string) string {
	t.Helper()

	id := ulid.MustNew(ulid.Timestamp(ts), ulid.Monotonic(strings.NewReader(strings.Repeat("a", 64)), 0)).String()
	meta := Meta{
		ID:      id,
		TS:      ts.UTC().Format(time.RFC3339Nano),
		Size:    int64(len(data)),
		Preview: buildPreviewData(data, len(data)),
		Attrs:   mapsClone(attrs),
	}

	entryDir := filepath.Join(root, "entries", id)
	if err := os.MkdirAll(entryDir, 0o700); err != nil {
		t.Fatalf("mkdir imported entry: %v", err)
	}
	if err := os.WriteFile(filepath.Join(entryDir, "data"), data, 0o600); err != nil {
		t.Fatalf("write imported data: %v", err)
	}
	metaData, err := json.MarshalIndent(meta, "", "  ")
	if err != nil {
		t.Fatalf("marshal imported meta: %v", err)
	}
	if err := os.WriteFile(filepath.Join(entryDir, "meta.json"), metaData, 0o600); err != nil {
		t.Fatalf("write imported meta: %v", err)
	}
	return id
}

type failAfterWriter struct {
	limit int
	n     int
}

func (w *failAfterWriter) Write(p []byte) (int, error) {
	remaining := w.limit - w.n
	if remaining <= 0 {
		return 0, errors.New("forced write failure")
	}
	if len(p) > remaining {
		w.n += remaining
		return remaining, errors.New("forced write failure")
	}
	w.n += len(p)
	return len(p), nil
}

func TestIndexRebuildIncludesImportedOlderEntries(t *testing.T) {
	root := setupTempStash(t)

	id1, err := Push(strings.NewReader("one"), nil)
	if err != nil {
		t.Fatalf("push 1: %v", err)
	}
	id2, err := Push(strings.NewReader("two"), nil)
	if err != nil {
		t.Fatalf("push 2: %v", err)
	}

	olderID := createImportedEntry(t, root, time.Now().Add(-24*time.Hour), randomBytes(t, 257), map[string]string{"filename": "older.bin"})

	got, err := NthNewest(3)
	if err != nil {
		t.Fatalf("NthNewest(3): %v", err)
	}
	if got.ID != olderID {
		t.Fatalf("NthNewest(3) = %s, want %s", got.ID, olderID)
	}

	resolved, err := Resolve("@3")
	if err != nil {
		t.Fatalf("Resolve(@3): %v", err)
	}
	if resolved != olderID {
		t.Fatalf("Resolve(@3) = %s, want %s", resolved, olderID)
	}

	entries, err := List()
	if err != nil {
		t.Fatalf("List: %v", err)
	}
	if len(entries) != 3 {
		t.Fatalf("List len = %d, want 3", len(entries))
	}
	if entries[0].ID != id2 || entries[1].ID != id1 || entries[2].ID != olderID {
		t.Fatalf("List order = [%s %s %s], want [%s %s %s]", entries[0].ID, entries[1].ID, entries[2].ID, id2, id1, olderID)
	}
}

func TestOlderThanIDsAndRemove(t *testing.T) {
	setupTempStash(t)

	id1, err := Push(strings.NewReader("one"), nil)
	if err != nil {
		t.Fatalf("push 1: %v", err)
	}
	id2, err := Push(strings.NewReader("two"), nil)
	if err != nil {
		t.Fatalf("push 2: %v", err)
	}
	id3, err := Push(strings.NewReader("three"), nil)
	if err != nil {
		t.Fatalf("push 3: %v", err)
	}

	older, err := OlderThanIDs(id2)
	if err != nil {
		t.Fatalf("OlderThanIDs: %v", err)
	}
	if len(older) != 1 || older[0] != id1 {
		t.Fatalf("OlderThanIDs(%s) = %v, want [%s]", id2, older, id1)
	}

	if err := Remove(id1); err != nil {
		t.Fatalf("Remove: %v", err)
	}

	got, err := Resolve("@2")
	if err != nil {
		t.Fatalf("Resolve(@2): %v", err)
	}
	if got != id2 {
		t.Fatalf("Resolve(@2) = %s, want %s", got, id2)
	}

	entries, err := List()
	if err != nil {
		t.Fatalf("List: %v", err)
	}
	if len(entries) != 2 || entries[0].ID != id3 || entries[1].ID != id2 {
		t.Fatalf("List after remove = %#v, want ids [%s %s]", entries, id3, id2)
	}
}

func TestTeeSuccess(t *testing.T) {
	setupTempStash(t)

	var out strings.Builder
	id, err := Tee(strings.NewReader("alpha\nbeta\n"), &out, map[string]string{"job": "test"}, false)
	if err != nil {
		t.Fatalf("Tee: %v", err)
	}
	if out.String() != "alpha\nbeta\n" {
		t.Fatalf("stdout copy = %q", out.String())
	}
	meta, err := GetMeta(id)
	if err != nil {
		t.Fatalf("GetMeta: %v", err)
	}
	if meta.Attrs["job"] != "test" {
		t.Fatalf("meta job = %q, want test", meta.Attrs["job"])
	}
	if strings.TrimSpace(meta.Preview) == "" {
		t.Fatal("expected preview to be stored in meta.json")
	}
}

func TestTeeInterruptedDiscardedWithoutPartial(t *testing.T) {
	setupTempStash(t)

	writer := &failAfterWriter{limit: 8}
	_, err := Tee(strings.NewReader("alpha\nbeta\n"), writer, nil, false)
	if err == nil {
		t.Fatal("expected tee error")
	}

	entries, err := List()
	if err != nil {
		t.Fatalf("List: %v", err)
	}
	if len(entries) != 0 {
		t.Fatalf("expected no entries, got %d", len(entries))
	}
}

func TestTeeInterruptedSavedWithPartial(t *testing.T) {
	setupTempStash(t)

	writer := &failAfterWriter{limit: 8}
	id, err := Tee(strings.NewReader("alpha\nbeta\n"), writer, map[string]string{"job": "test"}, true)
	if err == nil {
		t.Fatal("expected partial tee error")
	}
	var partial *ErrPartialSaved
	if !errors.As(err, &partial) {
		t.Fatalf("expected ErrPartialSaved, got %T", err)
	}
	if partial.ID != id {
		t.Fatalf("partial id = %q, want %q", partial.ID, id)
	}
	meta, err := GetMeta(id)
	if err != nil {
		t.Fatalf("GetMeta: %v", err)
	}
	if meta.Attrs["partial"] != "true" {
		t.Fatalf("partial attr = %q, want true", meta.Attrs["partial"])
	}
	if meta.Attrs["job"] != "test" {
		t.Fatalf("job attr = %q, want test", meta.Attrs["job"])
	}
}

func TestListCacheRebuildsAndInvalidates(t *testing.T) {
	setupTempStash(t)

	id, err := Push(strings.NewReader("one"), map[string]string{"label": "first"})
	if err != nil {
		t.Fatalf("Push: %v", err)
	}

	items, err := List()
	if err != nil {
		t.Fatalf("List: %v", err)
	}
	if len(items) != 1 {
		t.Fatalf("List len = %d, want 1", len(items))
	}

	cachePath, err := listCachePath()
	if err != nil {
		t.Fatalf("listCachePath: %v", err)
	}
	if _, err := os.Stat(cachePath); err != nil {
		t.Fatalf("expected cache file: %v", err)
	}

	if err := SetAttrs(id, map[string]string{"label": "updated"}); err != nil {
		t.Fatalf("SetAttrs: %v", err)
	}
	items, err = List()
	if err != nil {
		t.Fatalf("List after SetAttrs: %v", err)
	}
	if items[0].Attrs["label"] != "updated" {
		t.Fatalf("cached attrs = %#v", items[0].Attrs)
	}

	if err := Remove(id); err != nil {
		t.Fatalf("Remove: %v", err)
	}
	items, err = List()
	if err != nil {
		t.Fatalf("List after Remove: %v", err)
	}
	if len(items) != 0 {
		t.Fatalf("List len after remove = %d, want 0", len(items))
	}
}
