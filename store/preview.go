package store

import (
	"fmt"
	"io"
	stdmime "mime"
	"os"
	"path/filepath"
	"strings"

	"github.com/gabriel-vasile/mimetype"
)

func isPreviewTextType(major, sub string) bool {
	return major == "text" || (major == "application" && sub == "json")
}

func mimeLabel(major, sub string) string {
	switch {
	case major == "" && sub == "":
		return ""
	case major == "":
		return sub
	case sub == "":
		return major
	default:
		return major + "/" + sub
	}
}

func detectMIMEParts(buf []byte) (major, sub string) {
	if len(buf) == 0 {
		return "application", "octet-stream"
	}
	mt := mimetype.Detect(buf)
	label := mt.String()
	if base, _, err := stdmime.ParseMediaType(label); err == nil {
		label = base
	}
	major, sub, _ = strings.Cut(strings.TrimSpace(strings.ToLower(label)), "/")
	if major == "" && sub != "" {
		major, sub = sub, ""
	}
	return major, sub
}

func buildPreviewData(buf []byte, chars int) (mimeType, preview string) {
	if len(buf) == 0 {
		return "application/octet-stream", "[empty]"
	}
	major, sub := detectMIMEParts(buf)
	mimeType = mimeLabel(major, sub)
	switch {
	case isPreviewTextType(major, sub):
		preview = buildTextPreview(buf, chars)
	default:
		preview = ""
	}
	return mimeType, preview
}

// SmartPreview reads up to chars bytes from an entry and returns the detected
// content type and a human-readable preview string.
func SmartPreview(id string, chars int) (mimeType, preview string, err error) {
	buf, err := readSample(id, chars)
	if err != nil {
		return "", "", err
	}
	mimeType, preview = buildPreviewData(buf, chars)
	if preview == "" && mimeType != "application/octet-stream" {
		preview = fmt.Sprintf("[%s]", mimeType)
	}
	return mimeType, preview, nil
}

// LongPreview returns up to maxLines of text from an entry for verbose display.
// Non-text content returns no preview lines.
func LongPreview(id string, charsPerLine, maxLines int) ([]string, error) {
	buf, err := readSample(id, charsPerLine*maxLines)
	if err != nil {
		return nil, err
	}

	if len(buf) == 0 {
		return nil, nil
	}

	major, sub := detectMIMEParts(buf)
	if !isPreviewTextType(major, sub) {
		return nil, nil
	}

	raw := string(buf)
	// Normalise line endings.
	raw = strings.ReplaceAll(raw, "\r\n", "\n")
	raw = strings.ReplaceAll(raw, "\r", "\n")

	var lines []string
	for _, line := range strings.SplitN(raw, "\n", maxLines+1) {
		if len(lines) == maxLines {
			break
		}
		// Replace non-printable chars.
		line = strings.Map(func(r rune) rune {
			if r == '\t' {
				return ' '
			}
			if r < 0x20 || r > 0x7e {
				return -1
			}
			return r
		}, line)
		lines = append(lines, line)
	}
	return lines, nil
}

// readSample reads up to n bytes from an entry's data file.
// Always reads at least 512 bytes so type detection has enough signal.
func readSample(id string, n int) ([]byte, error) {
	bufSize := n
	if bufSize < 512 {
		bufSize = 512
	}
	ed, err := entriesDir()
	if err != nil {
		return nil, err
	}
	f, err := os.Open(filepath.Join(ed, id, "data"))
	if err != nil {
		return nil, err
	}
	defer f.Close()

	buf := make([]byte, bufSize)
	nr, err := io.ReadFull(f, buf)
	if err != nil && err != io.ErrUnexpectedEOF && err != io.EOF {
		return nil, err
	}
	return buf[:nr], nil
}

func buildTextPreview(buf []byte, chars int) string {
	if len(buf) > chars {
		buf = buf[:chars]
	}
	var b strings.Builder
	lastSpace := true
	for _, r := range string(buf) {
		switch {
		case r == '\n':
			if b.Len() > 0 && !lastSpace {
				b.WriteByte(' ')
			}
			b.WriteString("⏎")
			b.WriteByte(' ')
			lastSpace = true
		case r == '\t' || r == '\r' || r == ' ':
			if !lastSpace {
				b.WriteByte(' ')
				lastSpace = true
			}
		case r < 0x20 || r > 0x7e:
			// Drop other non-printable and non-ASCII runes from compact preview.
		default:
			b.WriteRune(r)
			lastSpace = false
		}
	}
	return strings.TrimSpace(b.String())
}
