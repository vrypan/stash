package store

import (
	"fmt"
	"math/rand"
	"path/filepath"
	"strings"
	"testing"
)

func setupBenchmarkStash(b *testing.B, entries, payloadSize int) []string {
	b.Helper()

	root := filepath.Join(b.TempDir(), "stash")
	b.Setenv("STASH_DIR", root)
	if err := Init(); err != nil {
		b.Fatalf("Init: %v", err)
	}

	rng := rand.New(rand.NewSource(1))
	ids := make([]string, 0, entries)
	for i := 0; i < entries; i++ {
		payload := make([]byte, payloadSize)
		for j := range payload {
			payload[j] = byte('a' + rng.Intn(26))
		}
		id, err := Push(strings.NewReader(string(payload)), map[string]string{
			"filename": fmt.Sprintf("file-%04d.txt", i),
		})
		if err != nil {
			b.Fatalf("Push(%d): %v", i, err)
		}
		ids = append(ids, id)
	}
	if _, err := UpdateIndex(); err != nil {
		b.Fatalf("UpdateIndex: %v", err)
	}
	return ids
}

func BenchmarkList100(b *testing.B) {
	setupBenchmarkStash(b, 100, 256)
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		entries, err := List()
		if err != nil {
			b.Fatal(err)
		}
		if len(entries) != 100 {
			b.Fatalf("len(entries) = %d, want 100", len(entries))
		}
	}
}

func BenchmarkList1000(b *testing.B) {
	setupBenchmarkStash(b, 1000, 256)
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		entries, err := List()
		if err != nil {
			b.Fatal(err)
		}
		if len(entries) != 1000 {
			b.Fatalf("len(entries) = %d, want 1000", len(entries))
		}
	}
}

func BenchmarkResolveAt1_1000(b *testing.B) {
	setupBenchmarkStash(b, 1000, 256)
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		id, err := Resolve("@1")
		if err != nil {
			b.Fatal(err)
		}
		if id == "" {
			b.Fatal("empty id")
		}
	}
}

func BenchmarkResolveAt500_1000(b *testing.B) {
	setupBenchmarkStash(b, 1000, 256)
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		id, err := Resolve("@500")
		if err != nil {
			b.Fatal(err)
		}
		if id == "" {
			b.Fatal("empty id")
		}
	}
}

func BenchmarkNthNewest1000(b *testing.B) {
	setupBenchmarkStash(b, 1000, 256)
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		m, err := NthNewest(500)
		if err != nil {
			b.Fatal(err)
		}
		if m.ID == "" {
			b.Fatal("empty id")
		}
	}
}

func BenchmarkOlderThanIDs1000(b *testing.B) {
	ids := setupBenchmarkStash(b, 1000, 256)
	target := ids[len(ids)/2]
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		older, err := OlderThanIDs(target)
		if err != nil {
			b.Fatal(err)
		}
		if len(older) != 500 {
			b.Fatalf("len(older) = %d, want 500", len(older))
		}
	}
}

func BenchmarkUpdateIndex1000(b *testing.B) {
	setupBenchmarkStash(b, 1000, 256)
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		n, err := UpdateIndex()
		if err != nil {
			b.Fatal(err)
		}
		if n != 1000 {
			b.Fatalf("indexed = %d, want 1000", n)
		}
	}
}

func BenchmarkStreamSummariesFirst20_1000(b *testing.B) {
	setupBenchmarkStash(b, 1000, 256)
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		count := 0
		if err := StreamSummaries(func(s Summary) (bool, error) {
			if s.ID == "" {
				b.Fatal("empty id")
			}
			count++
			return count < 20, nil
		}); err != nil {
			b.Fatal(err)
		}
		if count != 20 {
			b.Fatalf("count = %d, want 20", count)
		}
	}
}

func BenchmarkStreamSummariesFirst20WithPreview_1000(b *testing.B) {
	setupBenchmarkStash(b, 1000, 256)
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		count := 0
		if err := StreamSummaries(func(s Summary) (bool, error) {
			if s.Preview == "" {
				b.Fatal("empty preview")
			}
			count++
			return count < 20, nil
		}); err != nil {
			b.Fatal(err)
		}
		if count != 20 {
			b.Fatalf("count = %d, want 20", count)
		}
	}
}
