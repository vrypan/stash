package store

import (
	"io"
	"os"
	"path/filepath"
	"strings"
	"unicode"
	"unicode/utf8"
)

const maxStoredPreviewRunes = 128

func buildPreviewData(buf []byte, chars int) string {
	if len(buf) == 0 {
		return ""
	}
	limit := maxStoredPreviewRunes
	if chars > 0 && chars < limit {
		limit = chars
	}
	return buildTextPreview(buf, limit)
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
	var last rune
	var haveLast bool
	for len(buf) > 0 {
		r, size := utf8.DecodeRune(buf)
		if r == utf8.RuneError && size == 1 {
			r = '.'
		}
		switch r {
		case '\n', '\r', '\t':
			r = ' '
		default:
			if !unicode.IsPrint(r) || unicode.IsControl(r) {
				r = '.'
			}
		}
		if haveLast && r == ' ' && last == ' ' {
			buf = buf[size:]
			continue
		}
		b.WriteRune(r)
		last = r
		haveLast = true
		runes++
		if chars > 0 && runes >= chars {
			break
		}
		buf = buf[size:]
	}
	return strings.TrimSpace(b.String())
}
