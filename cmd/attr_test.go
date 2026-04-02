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
