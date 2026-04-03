package store

import (
	"strings"
	"testing"
)

func TestBuildPreviewDataSanitizesAndCollapses(t *testing.T) {
	buf := []byte("alpha\t\tbeta\n\n\xff\xfe\x00gamma....    delta")

	got := buildPreviewData(buf, 512)

	if strings.Contains(got, "\t") || strings.Contains(got, "\n") || strings.Contains(got, "\r") {
		t.Fatalf("preview contains raw whitespace controls: %q", got)
	}
	if strings.Contains(got, "  ") {
		t.Fatalf("preview contains repeated spaces: %q", got)
	}
	if !strings.Contains(got, "alpha beta ...gamma.... delta") {
		t.Fatalf("preview = %q", got)
	}
}

func TestBuildPreviewDataCapsAt128Runes(t *testing.T) {
	got := buildPreviewData([]byte(strings.Repeat("a", 256)), 512)
	if len([]rune(got)) != maxStoredPreviewRunes {
		t.Fatalf("preview rune len = %d, want %d", len([]rune(got)), maxStoredPreviewRunes)
	}
}
