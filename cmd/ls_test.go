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

func TestLsTypeAndSubtypeColumns(t *testing.T) {
	setupTempCmdStash(t)
	if _, err := store.Push(strings.NewReader(`{"ok":true}`), nil); err != nil {
		t.Fatalf("store.Push: %v", err)
	}

	cmd := newLsCmd()
	cmd.SetArgs([]string{"--type", "--subtype"})
	stdout, _, err := captureIO(t, "", cmd.Execute)
	if err != nil {
		t.Fatalf("ls --type --subtype execute: %v", err)
	}
	if !strings.Contains(stdout, "application") || !strings.Contains(stdout, "json") {
		t.Fatalf("ls --type --subtype output = %q", stdout)
	}
}
