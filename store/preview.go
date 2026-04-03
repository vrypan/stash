package store

import (
	"io"
	"os"
	"path/filepath"
	"strings"
	"unicode"
	"unicode/utf8"
)

func buildPreviewData(buf []byte, chars int) string {
	if len(buf) == 0 {
		return ""
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

	raw := buildPreviewData(buf, len(buf))

	var lines []string
	for _, line := range strings.SplitN(raw, "\n", maxLines+1) {
		if len(lines) == maxLines {
			break
		}
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
	var b strings.Builder
	runes := 0
	for len(buf) > 0 {
		r, size := utf8.DecodeRune(buf)
		if r == utf8.RuneError && size == 1 {
			r = utf8.RuneError
		}
		switch r {
		case '\n', '\r', '\t':
			r = ' '
		default:
			if !unicode.IsPrint(r) || unicode.IsControl(r) {
				r = '.'
			}
		}
		b.WriteRune(r)
		runes++
		if chars > 0 && runes >= chars {
			break
		}
		buf = buf[size:]
	}
	return strings.TrimSpace(b.String())
}
