package store

import (
	"bufio"
	crand "crypto/rand"
	"encoding/gob"
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
const listCacheVersion = 1

// entropy is a monotonic ULID entropy source backed by crypto/rand.
var entropy = ulid.Monotonic(crand.Reader, 0)

// Meta holds per-entry attributes stored in attr.
type Meta struct {
	ID      string            `json:"id"`
	TS      string            `json:"ts"`
	Size    int64             `json:"size"`
	Preview string            `json:"preview,omitempty"`
	Attrs   map[string]string `json:"attr,omitempty"`
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

func cacheDir() (string, error) {
	b, err := BaseDir()
	if err != nil {
		return "", err
	}
	return filepath.Join(b, "cache"), nil
}

// Init creates the base directory structure.
func Init() error {
	b, err := BaseDir()
	if err != nil {
		return err
	}
	for _, dir := range []string{
		filepath.Join(b, "entries"),
		filepath.Join(b, "cache"),
		filepath.Join(b, "tmp"),
	} {
		if err := os.MkdirAll(dir, 0700); err != nil {
			return err
		}
	}
	return nil
}

func newULID() string {
	return ulid.MustNew(ulid.Timestamp(time.Now()), entropy).String()
}

type sampleWriter struct {
	buf []byte
	max int
}

type listCache struct {
	Version           int
	EntriesDirModTime int64
	Items             []Meta
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

func listCachePath() (string, error) {
	d, err := cacheDir()
	if err != nil {
		return "", err
	}
	return filepath.Join(d, "list.gob"), nil
}

func entriesDirModTime() (int64, error) {
	ed, err := entriesDir()
	if err != nil {
		return 0, err
	}
	info, err := os.Stat(ed)
	if err != nil {
		if os.IsNotExist(err) {
			return 0, nil
		}
		return 0, err
	}
	return info.ModTime().UnixNano(), nil
}

func invalidateListCache() error {
	path, err := listCachePath()
	if err != nil {
		return err
	}
	err = os.Remove(path)
	if err != nil && !os.IsNotExist(err) {
		return err
	}
	return nil
}

func readListCache() ([]Meta, bool, error) {
	path, err := listCachePath()
	if err != nil {
		return nil, false, err
	}
	f, err := os.Open(path)
	if err != nil {
		if os.IsNotExist(err) {
			return nil, false, nil
		}
		return nil, false, err
	}
	defer f.Close()

	var cache listCache
	if err := gob.NewDecoder(f).Decode(&cache); err != nil {
		return nil, false, nil
	}
	if cache.Version != listCacheVersion {
		return nil, false, nil
	}
	modTime, err := entriesDirModTime()
	if err != nil {
		return nil, false, err
	}
	if cache.EntriesDirModTime != modTime {
		return nil, false, nil
	}
	return cache.Items, true, nil
}

func writeListCache(items []Meta) error {
	if err := Init(); err != nil {
		return err
	}
	modTime, err := entriesDirModTime()
	if err != nil {
		return err
	}
	path, err := listCachePath()
	if err != nil {
		return err
	}
	tmpPath := path + ".tmp"
	f, err := os.OpenFile(tmpPath, os.O_CREATE|os.O_WRONLY|os.O_TRUNC, 0o600)
	if err != nil {
		return err
	}
	cache := listCache{
		Version:           listCacheVersion,
		EntriesDirModTime: modTime,
		Items:             items,
	}
	if err := gob.NewEncoder(f).Encode(cache); err != nil {
		f.Close()
		_ = os.Remove(tmpPath)
		return err
	}
	if err := f.Close(); err != nil {
		_ = os.Remove(tmpPath)
		return err
	}
	return os.Rename(tmpPath, path)
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
	attrData := marshalAttr(m)
	if err := os.WriteFile(filepath.Join(entryTmp, "attr"), attrData, 0o600); err != nil {
		return err
	}

	ed, err := entriesDir()
	if err != nil {
		return err
	}
	if err := os.Rename(entryTmp, filepath.Join(ed, id)); err != nil {
		return fmt.Errorf("finalize entry: %w", err)
	}
	return invalidateListCache()
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
	if items, ok, err := readListCache(); err != nil {
		return nil, err
	} else if ok {
		return items, nil
	}
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
	if err := writeListCache(metas); err != nil {
		return nil, err
	}
	return metas, nil
}

func attrPath(id string) (string, error) {
	ed, err := entriesDir()
	if err != nil {
		return "", err
	}
	return filepath.Join(ed, id, "attr"), nil
}

// GetMeta loads attr for a resolved canonical entry ID.
func GetMeta(id string) (Meta, error) {
	path, err := attrPath(id)
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
	return unmarshalAttr(data)
}

func writeMeta(id string, m Meta) error {
	path, err := attrPath(id)
	if err != nil {
		return err
	}
	if err := os.WriteFile(path, marshalAttr(m), 0600); err != nil {
		return err
	}
	return invalidateListCache()
}

func marshalAttr(m Meta) []byte {
	var b strings.Builder
	writeAttrLine(&b, "id", m.ID)
	writeAttrLine(&b, "ts", m.TS)
	writeAttrLine(&b, "size", strconv.FormatInt(m.Size, 10))
	if strings.TrimSpace(m.Preview) != "" {
		writeAttrLine(&b, "preview", m.Preview)
	}
	if len(m.Attrs) > 0 {
		keys := make([]string, 0, len(m.Attrs))
		for k := range m.Attrs {
			keys = append(keys, k)
		}
		sort.Strings(keys)
		for _, k := range keys {
			writeAttrLine(&b, k, m.Attrs[k])
		}
	}
	return []byte(b.String())
}

func writeAttrLine(b *strings.Builder, key, value string) {
	b.WriteString(escapeAttr(key))
	b.WriteByte('=')
	b.WriteString(escapeAttr(value))
	b.WriteByte('\n')
}

func escapeAttr(s string) string {
	var b strings.Builder
	b.Grow(len(s))
	for _, r := range s {
		switch r {
		case '\\':
			b.WriteString(`\\`)
		case '\n':
			b.WriteString(`\n`)
		case '\r':
			b.WriteString(`\r`)
		case '\t':
			b.WriteString(`\t`)
		case '=':
			b.WriteString(`\=`)
		default:
			b.WriteRune(r)
		}
	}
	return b.String()
}

func unmarshalAttr(data []byte) (Meta, error) {
	var m Meta
	attrs := make(map[string]string)
	scanner := bufio.NewScanner(strings.NewReader(string(data)))
	for scanner.Scan() {
		line := strings.TrimSpace(scanner.Text())
		if line == "" {
			continue
		}
		key, value, ok := splitAttrLine(line)
		if !ok {
			return Meta{}, fmt.Errorf("invalid attr line %q", line)
		}
		key, err := unescapeAttr(key)
		if err != nil {
			return Meta{}, err
		}
		value, err = unescapeAttr(value)
		if err != nil {
			return Meta{}, err
		}
		switch {
		case key == "id":
			m.ID = value
		case key == "ts":
			m.TS = value
		case key == "size":
			n, err := strconv.ParseInt(value, 10, 64)
			if err != nil {
				return Meta{}, fmt.Errorf("invalid size %q", value)
			}
			m.Size = n
		case key == "preview":
			m.Preview = value
		default:
			attrs[key] = value
		}
	}
	if err := scanner.Err(); err != nil {
		return Meta{}, err
	}
	if len(attrs) > 0 {
		m.Attrs = attrs
	}
	return m, nil
}

func splitAttrLine(line string) (string, string, bool) {
	escaped := false
	for i := 0; i < len(line); i++ {
		switch line[i] {
		case '\\':
			escaped = !escaped
		case '=':
			if !escaped {
				return line[:i], line[i+1:], true
			}
			escaped = false
		default:
			escaped = false
		}
	}
	return "", "", false
}

func unescapeAttr(s string) (string, error) {
	var b strings.Builder
	b.Grow(len(s))
	escaped := false
	for _, r := range s {
		if !escaped {
			if r == '\\' {
				escaped = true
				continue
			}
			b.WriteRune(r)
			continue
		}
		switch r {
		case '\\':
			b.WriteRune('\\')
		case 'n':
			b.WriteRune('\n')
		case 'r':
			b.WriteRune('\r')
		case 't':
			b.WriteRune('\t')
		case '=':
			b.WriteRune('=')
		default:
			return "", fmt.Errorf("invalid attr escape \\%c", r)
		}
		escaped = false
	}
	if escaped {
		return "", fmt.Errorf("unterminated attr escape")
	}
	return b.String(), nil
}

// SetAttrs updates only the user-defined attribute map for a resolved canonical
// entry ID.
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

// UnsetAttrs removes keys from the user-defined attribute map for a resolved
// canonical entry ID.
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
	items, err := List()
	if err != nil {
		return Meta{}, err
	}
	if len(items) == 0 {
		return Meta{}, ErrEmpty
	}
	return items[0], nil
}

// NthNewest returns the nth newest entry, where n=1 is the most recent entry.
func NthNewest(n int) (Meta, error) {
	if n < 1 {
		return Meta{}, fmt.Errorf("invalid entry index %d (minimum 1)", n)
	}
	items, err := List()
	if err != nil {
		return Meta{}, err
	}
	if len(items) == 0 {
		return Meta{}, ErrEmpty
	}
	if n > len(items) {
		return Meta{}, fmt.Errorf("entry index %d out of range (%d entries)", n, len(items))
	}
	return items[n-1], nil
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
	return invalidateListCache()
}

// OlderThanIDs returns IDs older than the referenced canonical entry ID.
func OlderThanIDs(id string) ([]string, error) {
	items, err := List()
	if err != nil {
		return nil, err
	}
	for i, item := range items {
		if item.ID == id {
			out := make([]string, 0, len(items)-i-1)
			for _, older := range items[i+1:] {
				out = append(out, older.ID)
			}
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
