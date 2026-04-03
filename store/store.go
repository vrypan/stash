package store

import (
	crand "crypto/rand"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
	"time"

	"github.com/oklog/ulid/v2"
)

const shortIDLen = 8
const minIDLen = 6

// entropy is a monotonic ULID entropy source backed by crypto/rand.
var entropy = ulid.Monotonic(crand.Reader, 0)

// Meta holds per-entry metadata stored in meta.json.
type Meta struct {
	ID      string            `json:"id"`
	TS      string            `json:"ts"`
	Size    int64             `json:"size"`
	Preview string            `json:"preview,omitempty"`
	Attrs   map[string]string `json:"meta,omitempty"`
}

func (m Meta) ShortID() string {
	id := m.ID
	if len(id) >= shortIDLen {
		id = id[len(id)-shortIDLen:]
	}
	return strings.ToLower(id)
}

func (m Meta) DisplayID() string {
	return strings.ToLower(m.ID)
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
	fmt.Fprintf(&sb, "ambiguous id %q\nmatches:\n", strings.ToLower(e.Input))
	for _, m := range e.Matches {
		fmt.Fprintf(&sb, "  %s\n", strings.ToLower(m))
	}
	return strings.TrimRight(sb.String(), "\n")
}

type ErrPartialSaved struct {
	ID    string
	Cause error
}

func (e *ErrPartialSaved) Error() string {
	if e == nil {
		return ""
	}
	if e.Cause == nil {
		return fmt.Sprintf("partial entry saved as %q", strings.ToLower(e.ID))
	}
	return fmt.Sprintf("partial entry saved as %q: %v", strings.ToLower(e.ID), e.Cause)
}

func (e *ErrPartialSaved) Unwrap() error {
	if e == nil {
		return nil
	}
	return e.Cause
}

// Path helpers.

func BaseDir() (string, error) {
	if dir := strings.TrimSpace(os.Getenv("STASH_DIR")); dir != "" {
		return dir, nil
	}
	home, err := os.UserHomeDir()
	if err != nil {
		return "", err
	}
	return filepath.Join(home, ".stash"), nil
}

func BaseDirPath() (string, error) {
	b, err := BaseDir()
	if err != nil {
		return "", err
	}
	return filepath.Abs(b)
}

func entriesDir() (string, error) {
	b, err := BaseDir()
	if err != nil {
		return "", err
	}
	return filepath.Join(b, "entries"), nil
}

func EntriesDirPath() (string, error) {
	ed, err := entriesDir()
	if err != nil {
		return "", err
	}
	return filepath.Abs(ed)
}

func EntryDirPath(id string) (string, error) {
	ed, err := entriesDir()
	if err != nil {
		return "", err
	}
	return filepath.Abs(filepath.Join(ed, id))
}

func EntryDataPath(id string) (string, error) {
	ed, err := entriesDir()
	if err != nil {
		return "", err
	}
	return filepath.Abs(filepath.Join(ed, id, "data"))
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

type sampleWriter struct {
	buf []byte
	max int
}

func mapsClone(in map[string]string) map[string]string {
	if len(in) == 0 {
		return map[string]string{}
	}
	out := make(map[string]string, len(in))
	for k, v := range in {
		out[k] = v
	}
	return out
}

func (w *sampleWriter) Write(p []byte) (int, error) {
	if len(w.buf) < w.max {
		need := w.max - len(w.buf)
		if need > len(p) {
			need = len(p)
		}
		w.buf = append(w.buf, p[:need]...)
	}
	return len(p), nil
}

func prepareEntry() (id string, entryTmp string, cleanup func(), err error) {
	if err := Init(); err != nil {
		return "", "", nil, err
	}

	id = newULID()
	tmpDir, err := tmpStashDir()
	if err != nil {
		return "", "", nil, err
	}
	entryTmp = filepath.Join(tmpDir, id)
	if err := os.MkdirAll(entryTmp, 0o700); err != nil {
		return "", "", nil, err
	}
	cleanup = func() { os.RemoveAll(entryTmp) }
	return id, entryTmp, cleanup, nil
}

func finalizeEntry(id, entryTmp string, size int64, attrs map[string]string, sample []byte) error {
	if attrs == nil {
		attrs = map[string]string{}
	}
	attrs = mapsClone(attrs)

	m := Meta{
		ID:      id,
		TS:      time.Now().UTC().Format(time.RFC3339Nano),
		Size:    size,
		Preview: buildPreviewData(sample, len(sample)),
		Attrs:   attrs,
	}
	metaData, err := json.MarshalIndent(m, "", "  ")
	if err != nil {
		return err
	}
	if err := os.WriteFile(filepath.Join(entryTmp, "meta.json"), metaData, 0o600); err != nil {
		return err
	}

	ed, err := entriesDir()
	if err != nil {
		return err
	}
	if err := os.Rename(entryTmp, filepath.Join(ed, id)); err != nil {
		return fmt.Errorf("finalize entry: %w", err)
	}
	return nil
}

// Push reads r, stores it as a new entry, and returns the canonical ULID.
// attrs is an optional map of user-supplied key=value metadata.
func Push(r io.Reader, attrs map[string]string) (string, error) {
	id, entryTmp, cleanup, err := prepareEntry()
	if err != nil {
		return "", err
	}
	keep := false
	defer func() {
		if !keep {
			cleanup()
		}
	}()

	f, err := os.OpenFile(filepath.Join(entryTmp, "data"), os.O_CREATE|os.O_WRONLY|os.O_EXCL, 0600)
	if err != nil {
		return "", err
	}

	sample := &sampleWriter{max: 512}
	size, err := io.Copy(io.MultiWriter(f, sample), r)
	f.Close()
	if err != nil {
		return "", fmt.Errorf("write data: %w", err)
	}
	if err := finalizeEntry(id, entryTmp, size, attrs, sample.buf); err != nil {
		return "", err
	}
	keep = true
	return id, nil
}

// Tee streams stdin to w while also storing the received bytes as a new entry.
// On success it returns the new canonical ID.
// If partial is false, any stream error discards the temp entry.
// If partial is true and at least one byte was captured, the partial entry is
// finalized and ErrPartialSaved is returned.
func Tee(r io.Reader, w io.Writer, attrs map[string]string, partial bool) (string, error) {
	id, entryTmp, cleanup, err := prepareEntry()
	if err != nil {
		return "", err
	}
	keep := false
	defer func() {
		if !keep {
			cleanup()
		}
	}()

	f, err := os.OpenFile(filepath.Join(entryTmp, "data"), os.O_CREATE|os.O_WRONLY|os.O_EXCL, 0o600)
	if err != nil {
		return "", err
	}

	sample := &sampleWriter{max: 512}
	size, copyErr := io.Copy(io.MultiWriter(f, sample, w), r)
	closeErr := f.Close()
	if copyErr == nil && closeErr != nil {
		copyErr = closeErr
	}

	if copyErr != nil {
		if !partial || size == 0 {
			return "", fmt.Errorf("stream tee: %w", copyErr)
		}
		if attrs == nil {
			attrs = make(map[string]string, 1)
		}
		attrs["partial"] = "true"
		if err := finalizeEntry(id, entryTmp, size, attrs, sample.buf); err != nil {
			return "", err
		}
		keep = true
		return id, &ErrPartialSaved{ID: id, Cause: copyErr}
	}

	if err := finalizeEntry(id, entryTmp, size, attrs, sample.buf); err != nil {
		return "", err
	}
	keep = true
	return id, nil
}

func listEntryDirIDs() ([]string, error) {
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
	ids := make([]string, 0, len(dirEntries))
	for _, de := range dirEntries {
		if de.IsDir() {
			ids = append(ids, de.Name())
		}
	}
	sort.Sort(sort.Reverse(sort.StringSlice(ids)))
	return ids, nil
}

// List returns all entries sorted newest first.
func List() ([]Meta, error) {
	ids, err := listEntryDirIDs()
	if err != nil {
		return nil, err
	}
	metas := make([]Meta, 0, len(ids))
	for _, id := range ids {
		m, err := GetMeta(id)
		if err != nil {
			var notFound *ErrNotFound
			if errors.As(err, &notFound) {
				continue
			}
			return nil, err
		}
		metas = append(metas, m)
	}
	return metas, nil
}

func metaPath(id string) (string, error) {
	ed, err := entriesDir()
	if err != nil {
		return "", err
	}
	return filepath.Join(ed, id, "meta.json"), nil
}

// GetMeta loads meta.json for a resolved canonical entry ID.
func GetMeta(id string) (Meta, error) {
	path, err := metaPath(id)
	if err != nil {
		return Meta{}, err
	}
	data, err := os.ReadFile(path)
	if err != nil {
		if os.IsNotExist(err) {
			return Meta{}, &ErrNotFound{Input: id}
		}
		return Meta{}, err
	}
	var m Meta
	if err := json.Unmarshal(data, &m); err != nil {
		return Meta{}, err
	}
	return m, nil
}

func writeMeta(id string, m Meta) error {
	path, err := metaPath(id)
	if err != nil {
		return err
	}
	data, err := json.MarshalIndent(m, "", "  ")
	if err != nil {
		return err
	}
	if err := os.WriteFile(path, data, 0600); err != nil {
		return err
	}
	return nil
}

// SetAttrs updates only the user metadata map for a resolved canonical entry ID.
func SetAttrs(id string, attrs map[string]string) error {
	m, err := GetMeta(id)
	if err != nil {
		return err
	}
	if len(attrs) == 0 {
		return nil
	}
	if m.Attrs == nil {
		m.Attrs = make(map[string]string, len(attrs))
	}
	for k, v := range attrs {
		m.Attrs[k] = v
	}
	return writeMeta(id, m)
}

// UnsetAttrs removes keys from the user metadata map for a resolved canonical entry ID.
func UnsetAttrs(id string, keys []string) error {
	m, err := GetMeta(id)
	if err != nil {
		return err
	}
	if len(keys) == 0 || len(m.Attrs) == 0 {
		return nil
	}
	for _, k := range keys {
		delete(m.Attrs, k)
	}
	if len(m.Attrs) == 0 {
		m.Attrs = nil
	}
	return writeMeta(id, m)
}

// Newest returns the most recent entry or ErrEmpty.
func Newest() (Meta, error) {
	ids, err := listEntryDirIDs()
	if err != nil {
		return Meta{}, err
	}
	if len(ids) == 0 {
		return Meta{}, ErrEmpty
	}
	return GetMeta(ids[0])
}

// NthNewest returns the nth newest entry, where n=1 is the most recent entry.
func NthNewest(n int) (Meta, error) {
	if n < 1 {
		return Meta{}, fmt.Errorf("invalid entry index %d (minimum 1)", n)
	}
	ids, err := listEntryDirIDs()
	if err != nil {
		return Meta{}, err
	}
	if len(ids) == 0 {
		return Meta{}, ErrEmpty
	}
	if n > len(ids) {
		return Meta{}, fmt.Errorf("entry index %d out of range (%d entries)", n, len(ids))
	}
	return GetMeta(ids[n-1])
}

// Resolve resolves a user-supplied ID to a canonical ULID.
func Resolve(input string) (string, error) {
	rawInput := strings.TrimSpace(input)
	if strings.HasPrefix(rawInput, "@") {
		n, err := strconv.Atoi(strings.TrimPrefix(rawInput, "@"))
		if err != nil || n < 1 {
			return "", fmt.Errorf("invalid stack ref %q", rawInput)
		}
		m, err := NthNewest(n)
		if err != nil {
			return "", err
		}
		return m.ID, nil
	}

	input = strings.ToUpper(rawInput)
	if len(input) < minIDLen {
		return "", fmt.Errorf("id too short: %q (minimum %d characters)", input, minIDLen)
	}

	ids, err := listEntryDirIDs()
	if err != nil {
		return "", err
	}
	if len(ids) == 0 {
		return "", &ErrNotFound{Input: input}
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

// Preview reads up to n bytes from the entry and returns a sanitized preview.
func Preview(id string, n int) (string, error) {
	buf, err := readSample(id, n)
	if err != nil {
		return "", err
	}
	return buildPreviewData(buf, n), nil
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
	if err := os.RemoveAll(filepath.Join(ed, id)); err != nil {
		return err
	}
	return nil
}

// OlderThanIDs returns IDs older than the referenced canonical entry ID.
func OlderThanIDs(id string) ([]string, error) {
	ids, err := listEntryDirIDs()
	if err != nil {
		return nil, err
	}
	for i, cur := range ids {
		if cur == id {
			out := append([]string(nil), ids[i+1:]...)
			return out, nil
		}
	}
	return nil, &ErrNotFound{Input: id}
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
