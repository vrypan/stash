package cmd

import (
	crand "crypto/rand"
	"io"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/fatih/color"
	"stash/store"
)

func setupTempCmdStash(t *testing.T) string {
	t.Helper()
	root := filepath.Join(t.TempDir(), "stash")
	t.Setenv("STASH_DIR", root)
	if err := store.Init(); err != nil {
		t.Fatalf("store.Init: %v", err)
	}
	return root
}

func randomFile(t *testing.T, dir string, name string, size int) string {
	t.Helper()
	buf := make([]byte, size)
	if _, err := crand.Read(buf); err != nil {
		t.Fatalf("random file bytes: %v", err)
	}
	path := filepath.Join(dir, name)
	if err := os.WriteFile(path, buf, 0o600); err != nil {
		t.Fatalf("write random file: %v", err)
	}
	return path
}

func captureIO(t *testing.T, stdin string, fn func() error) (stdout, stderr string, err error) {
	t.Helper()

	oldStdout, oldStderr, oldStdin := os.Stdout, os.Stderr, os.Stdin
	oldColorOutput, oldNoColor := color.Output, color.NoColor

	outR, outW, err := os.Pipe()
	if err != nil {
		t.Fatalf("stdout pipe: %v", err)
	}
	errR, errW, err := os.Pipe()
	if err != nil {
		t.Fatalf("stderr pipe: %v", err)
	}

	inFile, err := os.CreateTemp(t.TempDir(), "stdin-*")
	if err != nil {
		t.Fatalf("stdin temp file: %v", err)
	}
	if _, err := inFile.WriteString(stdin); err != nil {
		t.Fatalf("write stdin temp file: %v", err)
	}
	if _, err := inFile.Seek(0, 0); err != nil {
		t.Fatalf("seek stdin temp file: %v", err)
	}

	os.Stdout = outW
	os.Stderr = errW
	os.Stdin = inFile
	color.Output = outW
	color.NoColor = true

	defer func() {
		os.Stdout = oldStdout
		os.Stderr = oldStderr
		os.Stdin = oldStdin
		color.Output = oldColorOutput
		color.NoColor = oldNoColor
		outR.Close()
		errR.Close()
		inFile.Close()
	}()

	err = fn()
	outW.Close()
	errW.Close()
	outBytes, _ := io.ReadAll(outR)
	errBytes, _ := io.ReadAll(errR)
	return string(outBytes), string(errBytes), err
}

func TestRunPushWithMetaStoresFilename(t *testing.T) {
	dir := t.TempDir()
	setupTempCmdStash(t)
	path := randomFile(t, dir, "sample.bin", 512)

	stdout, _, err := captureIO(t, "", func() error {
		return runPushWithMeta(nil, []string{path}, []string{"tag=test"})
	})
	if err != nil {
		t.Fatalf("runPushWithMeta: %v", err)
	}

	id, err := store.Resolve(strings.TrimSpace(stdout))
	if err != nil {
		t.Fatalf("Resolve pushed id: %v", err)
	}
	meta, err := store.GetMeta(id)
	if err != nil {
		t.Fatalf("GetMeta: %v", err)
	}
	if meta.Attrs["filename"] != "sample.bin" {
		t.Fatalf("filename attr = %q, want sample.bin", meta.Attrs["filename"])
	}
	if meta.Attrs["tag"] != "test" {
		t.Fatalf("tag attr = %q, want test", meta.Attrs["tag"])
	}
}

func TestLsPlainOutputsOnlyIDs(t *testing.T) {
	setupTempCmdStash(t)
	id1, err := store.Push(strings.NewReader("one"), nil)
	if err != nil {
		t.Fatalf("push 1: %v", err)
	}
	id2, err := store.Push(strings.NewReader("two"), nil)
	if err != nil {
		t.Fatalf("push 2: %v", err)
	}

	cmd := newLsCmd()
	cmd.SetArgs([]string{})
	stdout, _, err := captureIO(t, "", cmd.Execute)
	if err != nil {
		t.Fatalf("ls execute: %v", err)
	}
	meta2, err := store.GetMeta(id2)
	if err != nil {
		t.Fatalf("GetMeta 2: %v", err)
	}
	meta1, err := store.GetMeta(id1)
	if err != nil {
		t.Fatalf("GetMeta 1: %v", err)
	}
	want := meta2.ShortID() + "\n" + meta1.ShortID() + "\n"
	if stdout != want {
		t.Fatalf("ls output = %q, want %q", stdout, want)
	}

	cmd = newLsCmd()
	cmd.SetArgs([]string{"--id=pos"})
	stdout, _, err = captureIO(t, "", cmd.Execute)
	if err != nil {
		t.Fatalf("ls --id=pos execute: %v", err)
	}
	if stdout != "1\n2\n" {
		t.Fatalf("ls --id=pos output = %q", stdout)
	}
}

func TestInspectFormatOutputsHash(t *testing.T) {
	setupTempCmdStash(t)
	id, err := store.Push(strings.NewReader("inspect me"), map[string]string{"filename": "inspect.txt"})
	if err != nil {
		t.Fatalf("store.Push: %v", err)
	}
	meta, err := store.GetMeta(id)
	if err != nil {
		t.Fatalf("GetMeta: %v", err)
	}

	cmd := newInspectCmd()
	cmd.SetArgs([]string{id, "--format", "{{.Hash}}", "--no-color"})
	stdout, _, err := captureIO(t, "", cmd.Execute)
	if err != nil {
		t.Fatalf("inspect execute: %v", err)
	}
	if strings.TrimSpace(stdout) != meta.Hash {
		t.Fatalf("inspect hash output = %q, want %q", strings.TrimSpace(stdout), meta.Hash)
	}
}

func TestInspectShowsPreviewWhenAvailable(t *testing.T) {
	setupTempCmdStash(t)
	id, err := store.Push(strings.NewReader("alpha\nbeta\ngamma\n"), map[string]string{"filename": "inspect.txt"})
	if err != nil {
		t.Fatalf("store.Push: %v", err)
	}

	cmd := newInspectCmd()
	cmd.SetArgs([]string{id, "--no-color"})
	stdout, _, err := captureIO(t, "", cmd.Execute)
	if err != nil {
		t.Fatalf("inspect execute: %v", err)
	}
	if !strings.Contains(stdout, "alpha") {
		t.Fatalf("inspect output missing preview: %q", stdout)
	}
}

func TestRunRmBeforeForce(t *testing.T) {
	setupTempCmdStash(t)
	id1, err := store.Push(strings.NewReader("one"), nil)
	if err != nil {
		t.Fatalf("push 1: %v", err)
	}
	id2, err := store.Push(strings.NewReader("two"), nil)
	if err != nil {
		t.Fatalf("push 2: %v", err)
	}
	id3, err := store.Push(strings.NewReader("three"), nil)
	if err != nil {
		t.Fatalf("push 3: %v", err)
	}

	if err := runRmBefore("@2", true); err != nil {
		t.Fatalf("runRmBefore: %v", err)
	}

	entries, err := store.List()
	if err != nil {
		t.Fatalf("List: %v", err)
	}
	if len(entries) != 2 || entries[0].ID != id3 || entries[1].ID != id2 {
		t.Fatalf("entries after rm --before = %#v, want ids [%s %s]", entries, id3, id2)
	}
	if _, err := store.GetMeta(id1); err == nil {
		t.Fatalf("expected oldest entry %s to be removed", id1)
	}
}

func TestIndexUpdateCommand(t *testing.T) {
	root := setupTempCmdStash(t)
	if _, err := store.Push(strings.NewReader("one"), nil); err != nil {
		t.Fatalf("push 1: %v", err)
	}
	if _, err := store.Push(strings.NewReader("two"), nil); err != nil {
		t.Fatalf("push 2: %v", err)
	}

	olderDir := filepath.Join(root, "entries", "01ARZ3NDEKTSV4RRFFQ69G5FAV")
	if err := os.MkdirAll(olderDir, 0o700); err != nil {
		t.Fatalf("mkdir older dir: %v", err)
	}
	if err := os.WriteFile(filepath.Join(olderDir, "data"), []byte("old"), 0o600); err != nil {
		t.Fatalf("write older data: %v", err)
	}
	metaJSON := `{"id":"01ARZ3NDEKTSV4RRFFQ69G5FAV","ts":"2020-01-01T00:00:00Z","hash":"x","size":3,"meta":{"mimetype":"text","mimesubtype":"plain"}}`
	if err := os.WriteFile(filepath.Join(olderDir, "meta.json"), []byte(metaJSON), 0o600); err != nil {
		t.Fatalf("write older meta: %v", err)
	}

	cmd := newIndexCmd()
	cmd.SetArgs([]string{"update"})
	stdout, _, err := captureIO(t, "", cmd.Execute)
	if err != nil {
		t.Fatalf("index update execute: %v", err)
	}
	if !strings.Contains(stdout, "indexed 3 entries") {
		t.Fatalf("index update output = %q", stdout)
	}
}

func TestTeeCommandWritesStreamToStdoutAndIDToStderr(t *testing.T) {
	setupTempCmdStash(t)

	cmd := newTeeCmd()
	stdout, stderr, err := captureIO(t, "alpha\nbeta\n", func() error {
		cmd.SetArgs([]string{})
		return cmd.Execute()
	})
	if err != nil {
		t.Fatalf("tee execute: %v", err)
	}
	if stdout != "alpha\nbeta\n" {
		t.Fatalf("tee stdout = %q", stdout)
	}
	id := strings.TrimSpace(stderr)
	if id == "" {
		t.Fatal("expected id on stderr")
	}
	meta, err := store.GetMeta(strings.ToUpper(id))
	if err != nil {
		t.Fatalf("GetMeta(%q): %v", id, err)
	}
	if meta.Size != int64(len(stdout)) {
		t.Fatalf("meta.Size = %d, want %d", meta.Size, len(stdout))
	}
}
