package cmd

import (
	"fmt"
	"math/rand"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/fatih/color"
	"stash/store"
)

func setupBenchmarkCmdStash(b *testing.B, entries, payloadSize int) {
	b.Helper()

	root := filepath.Join(b.TempDir(), "stash")
	b.Setenv("STASH_DIR", root)
	if err := store.Init(); err != nil {
		b.Fatalf("store.Init: %v", err)
	}

	rng := rand.New(rand.NewSource(1))
	for i := 0; i < entries; i++ {
		payload := make([]byte, payloadSize)
		for j := range payload {
			payload[j] = byte('a' + rng.Intn(26))
		}
		if i%4 == 0 && len(payload) > 0 {
			payload[0] = 0xff
		}
		attrs := map[string]string{
			"filename": fmt.Sprintf("file-%04d.txt", i),
		}
		if i%3 == 0 {
			attrs["label"] = fmt.Sprintf("group-%d", i%7)
		}
		if _, err := store.Push(strings.NewReader(string(payload)), attrs); err != nil {
			b.Fatalf("store.Push(%d): %v", i, err)
		}
	}
}

func benchmarkCommandIO(b *testing.B, fn func() error) {
	b.Helper()

	oldStdout, oldStderr, oldStdin := os.Stdout, os.Stderr, os.Stdin
	oldColorOutput, oldNoColor := color.Output, color.NoColor

	stdoutFile, err := os.CreateTemp(b.TempDir(), "stdout-*")
	if err != nil {
		b.Fatalf("stdout temp file: %v", err)
	}
	stderrFile, err := os.CreateTemp(b.TempDir(), "stderr-*")
	if err != nil {
		b.Fatalf("stderr temp file: %v", err)
	}
	stdinFile, err := os.CreateTemp(b.TempDir(), "stdin-*")
	if err != nil {
		b.Fatalf("stdin temp file: %v", err)
	}

	os.Stdout = stdoutFile
	os.Stderr = stderrFile
	os.Stdin = stdinFile
	color.Output = stdoutFile
	color.NoColor = true

	defer func() {
		os.Stdout = oldStdout
		os.Stderr = oldStderr
		os.Stdin = oldStdin
		color.Output = oldColorOutput
		color.NoColor = oldNoColor
		stdoutFile.Close()
		stderrFile.Close()
		stdinFile.Close()
	}()

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		if err := stdoutFile.Truncate(0); err != nil {
			b.Fatalf("stdout truncate: %v", err)
		}
		if _, err := stdoutFile.Seek(0, 0); err != nil {
			b.Fatalf("stdout seek: %v", err)
		}
		if err := stderrFile.Truncate(0); err != nil {
			b.Fatalf("stderr truncate: %v", err)
		}
		if _, err := stderrFile.Seek(0, 0); err != nil {
			b.Fatalf("stderr seek: %v", err)
		}
		if _, err := stdinFile.Seek(0, 0); err != nil {
			b.Fatalf("stdin seek: %v", err)
		}
		if err := fn(); err != nil {
			b.Fatal(err)
		}
	}
}

func BenchmarkLs1000(b *testing.B) {
	setupBenchmarkCmdStash(b, 1000, 256)
	b.ReportAllocs()
	benchmarkCommandIO(b, func() error {
		cmd := newLsCmd()
		cmd.SetArgs([]string{"-l", "-n", "20"})
		return cmd.Execute()
	})
}

func BenchmarkLog1000(b *testing.B) {
	setupBenchmarkCmdStash(b, 1000, 256)
	b.ReportAllocs()
	benchmarkCommandIO(b, func() error {
		cmd := newLogCmd()
		cmd.SetArgs([]string{"-n", "20", "--no-color"})
		return cmd.Execute()
	})
}

func BenchmarkAttrNewest1000(b *testing.B) {
	setupBenchmarkCmdStash(b, 1000, 256)
	b.ReportAllocs()
	benchmarkCommandIO(b, func() error {
		cmd := newAttrCmd()
		cmd.SetArgs([]string{"@1", "--preview"})
		return cmd.Execute()
	})
}
