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
	if !strings.Contains(stdout, "size\t5\n") {
		t.Fatalf("attr output missing size: %q", stdout)
	}
	if !strings.Contains(stdout, "job\tnightly\n") || !strings.Contains(stdout, "owner\tci\n") {
		t.Fatalf("attr output missing attrs: %q", stdout)
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
	if strings.Contains(stdout, "\"attr\": {") || !strings.Contains(stdout, "\"job\": \"nightly\"") || !strings.Contains(stdout, "\"owner\": \"ci\"") {
		t.Fatalf("attr --json output = %q", stdout)
	}
}

func TestAttrCommandGetsSingleKey(t *testing.T) {
	setupTempCmdStash(t)
	id, err := store.Push(strings.NewReader("hello"), map[string]string{"source": "demo"})
	if err != nil {
		t.Fatalf("store.Push: %v", err)
	}
	cmd := newAttrCmd()
	cmd.SetArgs([]string{id, "source"})
	stdout, _, err := captureIO(t, "", cmd.Execute)
	if err != nil {
		t.Fatalf("attr source execute: %v", err)
	}
	if strings.TrimSpace(stdout) != "demo" {
		t.Fatalf("attr source output = %q", strings.TrimSpace(stdout))
	}
}

func TestAttrCommandSetAndUnsetMetaKeys(t *testing.T) {
	setupTempCmdStash(t)
	id, err := store.Push(strings.NewReader("hello"), nil)
	if err != nil {
		t.Fatalf("store.Push: %v", err)
	}

	cmd := newAttrCmd()
	cmd.SetArgs([]string{id, "set", "source=demo", "stage=raw"})
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
	cmd.SetArgs([]string{id, "unset", "stage"})
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
	cmd.SetArgs([]string{id, "set", "size=nope"})
	_, _, err = captureIO(t, "", cmd.Execute)
	if err == nil {
		t.Fatal("expected error when setting core field")
	}
}

func TestAttrCommandRejectsDottedWritableKeys(t *testing.T) {
	setupTempCmdStash(t)
	id, err := store.Push(strings.NewReader("hello"), nil)
	if err != nil {
		t.Fatalf("store.Push: %v", err)
	}

	cmd := newAttrCmd()
	cmd.SetArgs([]string{id, "set", "meta.source=demo"})
	_, _, err = captureIO(t, "", cmd.Execute)
	if err == nil {
		t.Fatal("expected error when setting dotted writable key")
	}
}

func TestAttrCommandAllowsHyphenInMiddleOfWritableKey(t *testing.T) {
	setupTempCmdStash(t)
	id, err := store.Push(strings.NewReader("hello"), nil)
	if err != nil {
		t.Fatalf("store.Push: %v", err)
	}

	cmd := newAttrCmd()
	cmd.SetArgs([]string{id, "set", "build-id=demo"})
	_, _, err = captureIO(t, "", cmd.Execute)
	if err != nil {
		t.Fatalf("attr set build-id execute: %v", err)
	}

	meta, err := store.GetMeta(id)
	if err != nil {
		t.Fatalf("GetMeta: %v", err)
	}
	if meta.Attrs["build-id"] != "demo" {
		t.Fatalf("build-id attr = %#v", meta.Attrs)
	}
}

func TestAttrCommandRejectsWritableKeysWithLeadingOrTrailingDash(t *testing.T) {
	setupTempCmdStash(t)
	id, err := store.Push(strings.NewReader("hello"), nil)
	if err != nil {
		t.Fatalf("store.Push: %v", err)
	}

	cmd := newAttrCmd()
	cmd.SetArgs([]string{id, "set", "-bad=demo"})
	_, _, err = captureIO(t, "", cmd.Execute)
	if err == nil {
		t.Fatal("expected error when setting key with leading dash")
	}

	cmd = newAttrCmd()
	cmd.SetArgs([]string{id, "set", "bad-=demo"})
	_, _, err = captureIO(t, "", cmd.Execute)
	if err == nil {
		t.Fatal("expected error when setting key with trailing dash")
	}
}

func TestAttrCommandPreviewFlagAndPseudoProperty(t *testing.T) {
	setupTempCmdStash(t)
	id, err := store.Push(strings.NewReader("alpha\nbeta\n"), nil)
	if err != nil {
		t.Fatalf("store.Push: %v", err)
	}

	cmd := newAttrCmd()
	cmd.SetArgs([]string{id, "--preview"})
	stdout, _, err := captureIO(t, "", cmd.Execute)
	if err != nil {
		t.Fatalf("attr --preview execute: %v", err)
	}
	if !strings.Contains(stdout, "preview\talpha") {
		t.Fatalf("attr --preview output = %q", stdout)
	}

	cmd = newAttrCmd()
	cmd.SetArgs([]string{id, "preview"})
	stdout, _, err = captureIO(t, "", cmd.Execute)
	if err != nil {
		t.Fatalf("attr preview execute: %v", err)
	}
	if !strings.Contains(stdout, "alpha") {
		t.Fatalf("attr preview output = %q", stdout)
	}
}
