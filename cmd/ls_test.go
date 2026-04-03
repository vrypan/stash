package cmd

import (
	"strings"
	"testing"

	"stash/store"
)

func TestLsLongEnablesDateSizeAndName(t *testing.T) {
	setupTempCmdStash(t)
	id, err := store.Push(strings.NewReader("hello"), map[string]string{"filename": "hello.txt"})
	if err != nil {
		t.Fatalf("store.Push: %v", err)
	}

	cmd := newLsCmd()
	cmd.SetArgs([]string{"-l"})
	stdout, _, err := captureIO(t, "", cmd.Execute)
	if err != nil {
		t.Fatalf("ls -l execute: %v", err)
	}
	shortID := strings.ToLower(id[len(id)-8:])
	if !strings.Contains(stdout, shortID) {
		t.Fatalf("ls -l output missing id: %q", stdout)
	}
	if !strings.Contains(stdout, "5B") {
		t.Fatalf("ls -l output missing size: %q", stdout)
	}
	if !strings.Contains(stdout, "hello.txt") {
		t.Fatalf("ls -l output missing name: %q", stdout)
	}
	if !strings.Contains(stdout, " +") {
		t.Fatalf("ls -l output missing absolute date: %q", stdout)
	}
}

func TestLsMetaWithoutArgumentShowsAttrsWithoutFiltering(t *testing.T) {
	setupTempCmdStash(t)
	if _, err := store.Push(strings.NewReader("one"), map[string]string{"label": "first"}); err != nil {
		t.Fatalf("store.Push first: %v", err)
	}
	if _, err := store.Push(strings.NewReader("two"), nil); err != nil {
		t.Fatalf("store.Push second: %v", err)
	}

	cmd := newLsCmd()
	cmd.SetArgs([]string{"--meta", "@"})
	stdout, _, err := captureIO(t, "", cmd.Execute)
	if err != nil {
		t.Fatalf("ls --meta @ execute: %v", err)
	}
	if !strings.Contains(stdout, "first") {
		t.Fatalf("ls --meta output missing attrs: %q", stdout)
	}
	lines := strings.Split(strings.TrimSpace(stdout), "\n")
	if len(lines) != 2 {
		t.Fatalf("ls --meta @ should not filter entries, got %d lines in %q", len(lines), stdout)
	}
}

func TestLsMetaFiltersByPresenceWithOrSemanticsAndColumns(t *testing.T) {
	setupTempCmdStash(t)
	if _, err := store.Push(strings.NewReader("one"), map[string]string{"label": "first"}); err != nil {
		t.Fatalf("store.Push first: %v", err)
	}
	if _, err := store.Push(strings.NewReader("two"), map[string]string{"note": "second"}); err != nil {
		t.Fatalf("store.Push second: %v", err)
	}
	if _, err := store.Push(strings.NewReader("three"), map[string]string{"label": "third", "note": "third-note"}); err != nil {
		t.Fatalf("store.Push third: %v", err)
	}

	cmd := newLsCmd()
	cmd.SetArgs([]string{"--meta", "label", "--meta", "note"})
	stdout, _, err := captureIO(t, "", cmd.Execute)
	if err != nil {
		t.Fatalf("ls --meta label --meta note execute: %v", err)
	}
	if !strings.Contains(stdout, "first") || !strings.Contains(stdout, "second") || !strings.Contains(stdout, "third-note") {
		t.Fatalf("ls --meta label --meta note output missing tag values: %q", stdout)
	}
	lines := strings.Split(strings.TrimSpace(stdout), "\n")
	if len(lines) != 3 {
		t.Fatalf("ls --meta label --meta note should match entries with either tag, got %d lines in %q", len(lines), stdout)
	}
	if strings.Contains(stdout, "<empty>") {
		t.Fatalf("ls --meta label --meta note should not show placeholder text, got %q", stdout)
	}
}

func TestLsMetaShortFlagAlias(t *testing.T) {
	setupTempCmdStash(t)
	if _, err := store.Push(strings.NewReader("one"), map[string]string{"label": "first"}); err != nil {
		t.Fatalf("store.Push: %v", err)
	}

	cmd := newLsCmd()
	cmd.SetArgs([]string{"-m", "@"})
	stdout, _, err := captureIO(t, "", cmd.Execute)
	if err != nil {
		t.Fatalf("ls -m @ execute: %v", err)
	}
	if !strings.Contains(stdout, "first") {
		t.Fatalf("ls -m @ output missing attrs: %q", stdout)
	}
}
