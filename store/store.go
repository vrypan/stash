package store

import (
	crand "crypto/rand"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"time"

	"github.com/oklog/ulid/v2"
	"lukechampine.com/blake3"
)

const shortIDLen = 8
const minIDLen = 6

// entropy is a monotonic ULID entropy source backed by crypto/rand.
var entropy = ulid.Monotonic(crand.Reader, 0)

// Meta holds per-entry metadata stored in meta.json.
type Meta struct {
	ID   string `json:"id"`
	TS   string `json:"ts"`
	Hash string `json:"hash"`
	Size int64  `json:"size"`
}

func (m Meta) ShortID() string {
	if len(m.ID) >= shortIDLen {
		return m.ID[len(m.ID)-shortIDLen:]
	}
	return m.ID
}

// Sentinel errors.
var ErrEmpty = errors.New("stash is empty")

type ErrNotFound struct{ Input string }

func (e *ErrNotFound) Error() string {
	return fmt.Sprintf("no entry matches %q", e.Input)
}

type ErrAmbiguous struct {
	Input   string
	Matches []string
}

func (e *ErrAmbiguous) Error() string {
	var sb strings.Builder
	fmt.Fprintf(&sb, "ambiguous id %q\nmatches:\n", e.Input)
	for _, m := range e.Matches {
		fmt.Fprintf(&sb, "  %s\n", m)
	}
	return strings.TrimRight(sb.String(), "\n")
}

// Path helpers.

func BaseDir() (string, error) {
	home, err := os.UserHomeDir()
	if err != nil {
		return "", err
	}
	return filepath.Join(home, ".stash"), nil
}

func entriesDir() (string, error) {
	b, err := BaseDir()
	if err != nil {
		return "", err
	}
	return filepath.Join(b, "entries"), nil
}

func tmpStashDir() (string, error) {
	b, err := BaseDir()
	if err != nil {
		return "", err
	}
	return filepath.Join(b, "tmp"), nil
}

func LockFilePath() (string, error) {
	b, err := BaseDir()
	if err != nil {
		return "", err
	}
	return filepath.Join(b, "lock"), nil
}

// Init creates the base directory structure.
func Init() error {
	b, err := BaseDir()
	if err != nil {
		return err
	}
	for _, dir := range []string{
		filepath.Join(b, "entries"),
		filepath.Join(b, "tmp"),
	} {
		if err := os.MkdirAll(dir, 0700); err != nil {
			return err
		}
	}
	lp, err := LockFilePath()
	if err != nil {
		return err
	}
	f, err := os.OpenFile(lp, os.O_CREATE|os.O_RDWR, 0600)
	if err != nil {
		return err
	}
	f.Close()
	return nil
}

func newULID() string {
	return ulid.MustNew(ulid.Timestamp(time.Now()), entropy).String()
}

// Push reads r, stores it as a new entry, and returns the canonical ULID.
func Push(r io.Reader) (string, error) {
	if err := Init(); err != nil {
		return "", err
	}

	id := newULID()

	tmpDir, err := tmpStashDir()
	if err != nil {
		return "", err
	}
	entryTmp := filepath.Join(tmpDir, id)
	if err := os.MkdirAll(entryTmp, 0700); err != nil {
		return "", err
	}

	cleanup := true
	defer func() {
		if cleanup {
			os.RemoveAll(entryTmp)
		}
	}()

	f, err := os.OpenFile(filepath.Join(entryTmp, "data"), os.O_CREATE|os.O_WRONLY|os.O_EXCL, 0600)
	if err != nil {
		return "", err
	}

	h := blake3.New(32, nil)
	size, err := io.Copy(io.MultiWriter(f, h), r)
	f.Close()
	if err != nil {
		return "", fmt.Errorf("write data: %w", err)
	}

	m := Meta{
		ID:   id,
		TS:   time.Now().UTC().Format(time.RFC3339Nano),
		Hash: hex.EncodeToString(h.Sum(nil)),
		Size: size,
	}
	metaData, err := json.MarshalIndent(m, "", "  ")
	if err != nil {
		return "", err
	}
	if err := os.WriteFile(filepath.Join(entryTmp, "meta.json"), metaData, 0600); err != nil {
		return "", err
	}

	ed, err := entriesDir()
	if err != nil {
		return "", err
	}
	if err := os.Rename(entryTmp, filepath.Join(ed, id)); err != nil {
		return "", fmt.Errorf("finalize entry: %w", err)
	}
	cleanup = false
	return id, nil
}

// List returns all entries sorted newest first.
func List() ([]Meta, error) {
	ed, err := entriesDir()
	if err != nil {
		return nil, err
	}
	dirEntries, err := os.ReadDir(ed)
	if err != nil {
		if os.IsNotExist(err) {
			return nil, nil
		}
		return nil, err
	}

	var metas []Meta
	for _, de := range dirEntries {
		if !de.IsDir() {
			continue
		}
		data, err := os.ReadFile(filepath.Join(ed, de.Name(), "meta.json"))
		if err != nil {
			continue // skip incomplete entries
		}
		var m Meta
		if err := json.Unmarshal(data, &m); err != nil {
			continue
		}
		metas = append(metas, m)
	}

	sort.Slice(metas, func(i, j int) bool {
		return metas[i].ID > metas[j].ID
	})
	return metas, nil
}

// Newest returns the most recent entry or ErrEmpty.
func Newest() (Meta, error) {
	entries, err := List()
	if err != nil {
		return Meta{}, err
	}
	if len(entries) == 0 {
		return Meta{}, ErrEmpty
	}
	return entries[0], nil
}

// Resolve resolves a user-supplied ID to a canonical ULID.
func Resolve(input string) (string, error) {
	input = strings.ToUpper(strings.TrimSpace(input))
	if len(input) < minIDLen {
		return "", fmt.Errorf("id too short: %q (minimum %d characters)", input, minIDLen)
	}

	ed, err := entriesDir()
	if err != nil {
		return "", err
	}
	dirEntries, err := os.ReadDir(ed)
	if err != nil {
		if os.IsNotExist(err) {
			return "", &ErrNotFound{Input: input}
		}
		return "", err
	}

	var ids []string
	for _, de := range dirEntries {
		if de.IsDir() {
			ids = append(ids, de.Name())
		}
	}

	// 1. Exact canonical match.
	for _, id := range ids {
		if id == input {
			return id, nil
		}
	}

	// 2. Canonical ULID prefix match.
	var prefixMatches []string
	for _, id := range ids {
		if strings.HasPrefix(id, input) {
			prefixMatches = append(prefixMatches, id)
		}
	}
	if len(prefixMatches) == 1 {
		return prefixMatches[0], nil
	}
	if len(prefixMatches) > 1 {
		return "", &ErrAmbiguous{Input: input, Matches: prefixMatches}
	}

	// 3. Short-ID suffix match.
	var suffixMatches []string
	for _, id := range ids {
		if strings.HasSuffix(id, input) {
			suffixMatches = append(suffixMatches, id)
		}
	}
	if len(suffixMatches) == 1 {
		return suffixMatches[0], nil
	}
	if len(suffixMatches) > 1 {
		return "", &ErrAmbiguous{Input: input, Matches: suffixMatches}
	}

	return "", &ErrNotFound{Input: input}
}

// Cat writes the entry's raw data to w.
func Cat(id string, w io.Writer) error {
	ed, err := entriesDir()
	if err != nil {
		return err
	}
	f, err := os.Open(filepath.Join(ed, id, "data"))
	if err != nil {
		return fmt.Errorf("open entry data: %w", err)
	}
	defer f.Close()
	_, err = io.Copy(w, f)
	return err
}

// Remove removes an entry directory.
func Remove(id string) error {
	ed, err := entriesDir()
	if err != nil {
		return err
	}
	return os.RemoveAll(filepath.Join(ed, id))
}

// Clear removes all entry directories.
func Clear() error {
	ed, err := entriesDir()
	if err != nil {
		return err
	}
	dirEntries, err := os.ReadDir(ed)
	if err != nil {
		if os.IsNotExist(err) {
			return nil
		}
		return err
	}
	for _, de := range dirEntries {
		if de.IsDir() {
			if err := os.RemoveAll(filepath.Join(ed, de.Name())); err != nil {
				return err
			}
		}
	}
	return nil
}

// HumanSize formats a byte count for human-readable display.
func HumanSize(n int64) string {
	switch {
	case n < 1024:
		return fmt.Sprintf("%dB", n)
	case n < 1024*1024:
		return fmt.Sprintf("%.1fK", float64(n)/1024)
	case n < 1024*1024*1024:
		return fmt.Sprintf("%.1fM", float64(n)/(1024*1024))
	default:
		return fmt.Sprintf("%.1fG", float64(n)/(1024*1024*1024))
	}
}
