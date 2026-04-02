package cmd

import (
	"strings"
	"testing"

	"stash/store"
)

func TestAttrCommandOutputsTabSeparatedAndJSON(t *testing.T) {
	setupTempCmdStash(t)
	id, err := store.Push(strings.NewReader("hello"), map[string]string{"owner": "ci", "job": "nightly"})
	if err != nil {
		t.Fatalf("store.Push: %v", err)
	}

	cmd := newAttrCmd()
	cmd.SetArgs([]string{id})
	stdout, _, err := captureIO(t, "", cmd.Execute)
	if err != nil {
		t.Fatalf("attr execute: %v", err)
	}
	if !strings.Contains(stdout, "id\t"+strings.ToLower(id)+"\n") {
		t.Fatalf("attr output missing id: %q", stdout)
	}
	if !strings.Contains(stdout, "hash\t") {
		t.Fatalf("attr output missing hash: %q", stdout)
	}
	if !strings.Contains(stdout, "size\t5\n") {
		t.Fatalf("attr output missing size: %q", stdout)
	}
	if !strings.Contains(stdout, "meta.job\tnightly\n") || !strings.Contains(stdout, "meta.owner\tci\n") {
		t.Fatalf("attr output missing meta attrs: %q", stdout)
	}

	cmd = newAttrCmd()
	cmd.SetArgs([]string{id, "--json"})
	stdout, _, err = captureIO(t, "", cmd.Execute)
	if err != nil {
		t.Fatalf("attr --json execute: %v", err)
	}
	if !strings.Contains(stdout, "\"id\": \""+id+"\"") {
		t.Fatalf("attr --json output missing id: %q", stdout)
	}
	if !strings.Contains(stdout, "\"meta\": {") || !strings.Contains(stdout, "\"job\": \"nightly\"") || !strings.Contains(stdout, "\"owner\": \"ci\"") {
		t.Fatalf("attr --json output = %q", stdout)
	}
}

func TestAttrCommandGetsSingleKey(t *testing.T) {
	setupTempCmdStash(t)
	id, err := store.Push(strings.NewReader("hello"), map[string]string{"source": "demo"})
	if err != nil {
		t.Fatalf("store.Push: %v", err)
	}
	meta, err := store.GetMeta(id)
	if err != nil {
		t.Fatalf("GetMeta: %v", err)
	}

	cmd := newAttrCmd()
	cmd.SetArgs([]string{id, "hash"})
	stdout, _, err := captureIO(t, "", cmd.Execute)
	if err != nil {
		t.Fatalf("attr hash execute: %v", err)
	}
	if strings.TrimSpace(stdout) != meta.Hash {
		t.Fatalf("attr hash output = %q, want %q", strings.TrimSpace(stdout), meta.Hash)
	}

	cmd = newAttrCmd()
	cmd.SetArgs([]string{id, "meta.source"})
	stdout, _, err = captureIO(t, "", cmd.Execute)
	if err != nil {
		t.Fatalf("attr meta.source execute: %v", err)
	}
	if strings.TrimSpace(stdout) != "demo" {
		t.Fatalf("attr meta.source output = %q", strings.TrimSpace(stdout))
	}
}

func TestAttrCommandSetAndUnsetMetaKeys(t *testing.T) {
	setupTempCmdStash(t)
	id, err := store.Push(strings.NewReader("hello"), nil)
	if err != nil {
		t.Fatalf("store.Push: %v", err)
	}

	cmd := newAttrCmd()
	cmd.SetArgs([]string{id, "set", "meta.source=demo", "meta.stage=raw"})
	_, _, err = captureIO(t, "", cmd.Execute)
	if err != nil {
		t.Fatalf("attr set execute: %v", err)
	}

	meta, err := store.GetMeta(id)
	if err != nil {
		t.Fatalf("GetMeta after set: %v", err)
	}
	if meta.Attrs["source"] != "demo" || meta.Attrs["stage"] != "raw" {
		t.Fatalf("attrs after set = %#v", meta.Attrs)
	}

	cmd = newAttrCmd()
	cmd.SetArgs([]string{id, "unset", "meta.stage"})
	_, _, err = captureIO(t, "", cmd.Execute)
	if err != nil {
		t.Fatalf("attr unset execute: %v", err)
	}

	meta, err = store.GetMeta(id)
	if err != nil {
		t.Fatalf("GetMeta after unset: %v", err)
	}
	if _, ok := meta.Attrs["stage"]; ok {
		t.Fatalf("stage attr still present: %#v", meta.Attrs)
	}
	if meta.Attrs["source"] != "demo" {
		t.Fatalf("source attr changed unexpectedly: %#v", meta.Attrs)
	}
}

func TestAttrCommandRejectsWritesToCoreFields(t *testing.T) {
	setupTempCmdStash(t)
	id, err := store.Push(strings.NewReader("hello"), nil)
	if err != nil {
		t.Fatalf("store.Push: %v", err)
	}

	cmd := newAttrCmd()
	cmd.SetArgs([]string{id, "set", "hash=nope"})
	_, _, err = captureIO(t, "", cmd.Execute)
	if err == nil {
		t.Fatal("expected error when setting core field")
	}
}
