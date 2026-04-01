package store

import (
	crand "crypto/rand"
	"encoding/hex"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/oklog/ulid/v2"
	"lukechampine.com/blake3"
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
	sum := blake3.Sum256(data)
	meta := Meta{
		ID:    id,
		TS:    ts.UTC().Format(time.RFC3339Nano),
		Hash:  hex.EncodeToString(sum[:]),
		Size:  int64(len(data)),
		Type:  detectContentType(data),
		MIME:  "application/octet-stream",
		Attrs: attrs,
	}
	if meta.Type == "text" || meta.Type == "json" {
		meta.MIME = "text/plain; charset=utf-8"
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

func TestOlderThanIDsAndRemoveUpdateIndex(t *testing.T) {
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

	count, err := UpdateIndex()
	if err != nil {
		t.Fatalf("UpdateIndex: %v", err)
	}
	if count != 2 {
		t.Fatalf("UpdateIndex count = %d, want 2", count)
	}

	entries, err := List()
	if err != nil {
		t.Fatalf("List: %v", err)
	}
	if len(entries) != 2 || entries[0].ID != id3 || entries[1].ID != id2 {
		t.Fatalf("List after remove = %#v, want ids [%s %s]", entries, id3, id2)
	}
}
