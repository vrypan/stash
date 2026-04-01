package store

import (
	"bytes"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"strings"
	"unicode"
)

func isPreviewTextType(typeStr string) bool {
	return typeStr == "text" || typeStr == "json"
}

func buildPreviewData(buf []byte, chars int) (typeStr, preview string) {
	if len(buf) == 0 {
		return "empty", "[empty]"
	}
	typeStr = detectContentType(buf)
	switch typeStr {
	case "text", "json":
		preview = buildTextPreview(buf, chars)
	default:
		preview = ""
	}
	return typeStr, preview
}

// SmartPreview reads up to chars bytes from an entry and returns the detected
// content type and a human-readable preview string.
func SmartPreview(id string, chars int) (typeStr, preview string, err error) {
	buf, err := readSample(id, chars)
	if err != nil {
		return "", "", err
	}
	typeStr, preview = buildPreviewData(buf, chars)
	if preview == "" && typeStr != "empty" {
		preview = fmt.Sprintf("[%s]", typeStr)
	}
	return typeStr, preview, nil
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

	typeStr := detectContentType(buf)
	if typeStr != "text" && typeStr != "json" {
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

func detectContentType(buf []byte) string {
	if len(buf) == 0 {
		return "empty"
	}

	// Magic byte signatures.
	switch {
	case len(buf) >= 2 && buf[0] == 0x1f && buf[1] == 0x8b:
		return "gzip"
	case len(buf) >= 4 && buf[0] == 0x28 && buf[1] == 0xb5 && buf[2] == 0x2f && buf[3] == 0xfd:
		return "zstd"
	case len(buf) >= 4 && buf[0] == 'P' && buf[1] == 'K' && buf[2] == 0x03 && buf[3] == 0x04:
		return "zip"
	case len(buf) >= 4 && buf[0] == 0x89 && buf[1] == 'P' && buf[2] == 'N' && buf[3] == 'G':
		return "png"
	case len(buf) >= 3 && buf[0] == 0xff && buf[1] == 0xd8 && buf[2] == 0xff:
		return "jpeg"
	case len(buf) >= 4 && string(buf[:4]) == "%PDF":
		return "pdf"
	case len(buf) >= 4 && string(buf[:4]) == "GIF8":
		return "gif"
	}

	// JSON: first non-whitespace char is { or [
	trimmed := bytes.TrimLeftFunc(buf, unicode.IsSpace)
	if len(trimmed) > 0 && (trimmed[0] == '{' || trimmed[0] == '[') {
		return "json"
	}

	// Text: ≥80% printable bytes.
	printable := 0
	for _, b := range buf {
		if (b >= 0x20 && b <= 0x7e) || b == '\n' || b == '\r' || b == '\t' {
			printable++
		}
	}
	if float64(printable)/float64(len(buf)) >= 0.80 {
		return "text"
	}
	return "binary"
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
