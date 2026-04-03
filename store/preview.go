package store

import (
	"io"
	"os"
	"path/filepath"
	"strings"
)

func buildPreviewData(buf []byte, chars int) string {
	if len(buf) == 0 {
		return "[empty]"
	}
	return buildTextPreview(buf, chars)
}

// SmartPreview reads up to chars bytes from an entry and returns a human-readable
// preview string built from the sampled bytes.
func SmartPreview(id string, chars int) (string, error) {
	buf, err := readSample(id, chars)
	if err != nil {
		return "", err
	}
	return buildPreviewData(buf, chars), nil
}

// LongPreview returns up to maxLines of preview text from an entry.
func LongPreview(id string, charsPerLine, maxLines int) ([]string, error) {
	buf, err := readSample(id, charsPerLine*maxLines)
	if err != nil {
		return nil, err
	}
	if len(buf) == 0 {
		return nil, nil
	}

	raw := string(buf)
	raw = strings.ReplaceAll(raw, "\r\n", "\n")
	raw = strings.ReplaceAll(raw, "\r", "\n")

	var lines []string
	for _, line := range strings.SplitN(raw, "\n", maxLines+1) {
		if len(lines) == maxLines {
			break
		}
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
// Always reads at least 512 bytes so previews have enough content.
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
