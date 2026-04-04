package cmd

import (
	"strings"
	"testing"

	"stash/store"
)

func TestLogMetaWithoutArgumentShowsMetaWithoutFiltering(t *testing.T) {
	setupTempCmdStash(t)
	if _, err := store.Push(strings.NewReader("one"), map[string]string{"label": "first"}); err != nil {
		t.Fatalf("store.Push first: %v", err)
	}
	if _, err := store.Push(strings.NewReader("two"), nil); err != nil {
		t.Fatalf("store.Push second: %v", err)
	}

	cmd := newLogCmd()
	cmd.SetArgs([]string{"--attr", "@", "--no-color"})
	stdout, _, err := captureIO(t, "", cmd.Execute)
	if err != nil {
		t.Fatalf("log --attr @ execute: %v", err)
	}
	if !strings.Contains(stdout, "label=first") {
		t.Fatalf("log --attr output missing attrs: %q", stdout)
	}
	if strings.Count(stdout, "entry ") != 2 {
		t.Fatalf("log --attr @ should not filter entries, got %q", stdout)
	}
}

func TestLogMetaFiltersByPresenceWithOrSemantics(t *testing.T) {
	setupTempCmdStash(t)
	if _, err := store.Push(strings.NewReader("one"), map[string]string{"label": "first"}); err != nil {
		t.Fatalf("store.Push first: %v", err)
	}
	if _, err := store.Push(strings.NewReader("two"), map[string]string{"note": "second"}); err != nil {
		t.Fatalf("store.Push second: %v", err)
	}
	if _, err := store.Push(strings.NewReader("three"), map[string]string{"other": "ignored"}); err != nil {
		t.Fatalf("store.Push third: %v", err)
	}

	cmd := newLogCmd()
	cmd.SetArgs([]string{"--attr", "label", "--attr", "note", "--no-color"})
	stdout, _, err := captureIO(t, "", cmd.Execute)
	if err != nil {
		t.Fatalf("log --attr label --attr note execute: %v", err)
	}
	if strings.Count(stdout, "entry ") != 2 {
		t.Fatalf("log --attr label --attr note should match entries with either tag, got %q", stdout)
	}
	if !strings.Contains(stdout, "label=first") || !strings.Contains(stdout, "note=second") {
		t.Fatalf("log --attr label --attr note output missing selected tags: %q", stdout)
	}
	if strings.Contains(stdout, "other=ignored") {
		t.Fatalf("log --attr label --attr note should not show unrelated tags: %q", stdout)
	}
}
