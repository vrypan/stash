package store

import (
	"bufio"
	crand "crypto/rand"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"os"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
	"time"

	"github.com/oklog/ulid/v2"
	"lukechampine.com/blake3"
)

const shortIDLen = 8
const minIDLen = 6
const indexVersion = 1

// entropy is a monotonic ULID entropy source backed by crypto/rand.
var entropy = ulid.Monotonic(crand.Reader, 0)

// Meta holds per-entry metadata stored in meta.json.
type Meta struct {
	ID    string            `json:"id"`
	TS    string            `json:"ts"`
	Hash  string            `json:"hash"`
	Size  int64             `json:"size"`
	Type  string            `json:"type,omitempty"`
	MIME  string            `json:"mime,omitempty"`
	Attrs map[string]string `json:"meta,omitempty"`
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

func indexDir() (string, error) {
	b, err := BaseDir()
	if err != nil {
		return "", err
	}
	return filepath.Join(b, "index"), nil
}

func entryIndexPath() (string, error) {
	d, err := indexDir()
	if err != nil {
		return "", err
	}
	return filepath.Join(d, "entries.json"), nil
}

func summaryIndexPath() (string, error) {
	d, err := indexDir()
	if err != nil {
		return "", err
	}
	return filepath.Join(d, "entries.ndjson"), nil
}

// Init creates the base directory structure.
func Init() error {
	b, err := BaseDir()
	if err != nil {
		return err
	}
	for _, dir := range []string{
		filepath.Join(b, "entries"),
		filepath.Join(b, "index"),
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

type entryIndex struct {
	Version           int      `json:"version"`
	EntriesDirModTime int64    `json:"entries_dir_mod_time"`
	IDs               []string `json:"ids"`
}

type summaryIndexHeader struct {
	Version           int   `json:"version"`
	EntriesDirModTime int64 `json:"entries_dir_mod_time"`
}

type summaryYieldFunc func(Meta) (bool, error)

type sampleWriter struct {
	buf []byte
	max int
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

// Push reads r, stores it as a new entry, and returns the canonical ULID.
// attrs is an optional map of user-supplied key=value metadata.
func Push(r io.Reader, attrs map[string]string) (string, error) {
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
	sample := &sampleWriter{max: 512}
	size, err := io.Copy(io.MultiWriter(f, h, sample), r)
	f.Close()
	if err != nil {
		return "", fmt.Errorf("write data: %w", err)
	}

	var typeStr, mimeStr string
	if len(sample.buf) > 0 {
		typeStr = detectContentType(sample.buf)
		mimeStr = http.DetectContentType(sample.buf)
	} else {
		typeStr = "empty"
		mimeStr = "application/octet-stream"
	}

	m := Meta{
		ID:    id,
		TS:    time.Now().UTC().Format(time.RFC3339Nano),
		Hash:  hex.EncodeToString(h.Sum(nil)),
		Size:  size,
		Type:  typeStr,
		MIME:  mimeStr,
		Attrs: attrs,
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
	if err := insertIndexedID(id); err != nil {
		return "", err
	}
	if err := invalidateSummaryIndex(); err != nil {
		return "", err
	}
	cleanup = false
	return id, nil
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

func listEntryDirIDs() ([]string, int64, error) {
	ed, err := entriesDir()
	if err != nil {
		return nil, 0, err
	}
	dirEntries, err := os.ReadDir(ed)
	if err != nil {
		if os.IsNotExist(err) {
			return nil, 0, nil
		}
		return nil, 0, err
	}
	ids := make([]string, 0, len(dirEntries))
	for _, de := range dirEntries {
		if de.IsDir() {
			ids = append(ids, de.Name())
		}
	}
	sort.Sort(sort.Reverse(sort.StringSlice(ids)))
	info, err := os.Stat(ed)
	if err != nil {
		return nil, 0, err
	}
	return ids, info.ModTime().UnixNano(), nil
}

func readEntryIndex() (entryIndex, error) {
	path, err := entryIndexPath()
	if err != nil {
		return entryIndex{}, err
	}
	data, err := os.ReadFile(path)
	if err != nil {
		return entryIndex{}, err
	}
	var idx entryIndex
	if err := json.Unmarshal(data, &idx); err != nil {
		return entryIndex{}, err
	}
	if idx.Version != indexVersion {
		return entryIndex{}, fmt.Errorf("unsupported index version %d", idx.Version)
	}
	return idx, nil
}

func writeEntryIndex(ids []string) error {
	if err := Init(); err != nil {
		return err
	}
	idx := entryIndex{
		Version: indexVersion,
		IDs:     append([]string(nil), ids...),
	}
	modTime, err := entriesDirModTime()
	if err != nil {
		return err
	}
	idx.EntriesDirModTime = modTime
	data, err := json.MarshalIndent(idx, "", "  ")
	if err != nil {
		return err
	}
	path, err := entryIndexPath()
	if err != nil {
		return err
	}
	return os.WriteFile(path, data, 0600)
}

func rebuildEntryIndex() ([]string, error) {
	ids, _, err := listEntryDirIDs()
	if err != nil {
		return nil, err
	}
	if err := writeEntryIndex(ids); err != nil {
		return nil, err
	}
	return ids, nil
}

func loadEntryIDs() ([]string, error) {
	idx, err := readEntryIndex()
	if err == nil {
		ed, dirErr := entriesDir()
		if dirErr != nil {
			return nil, dirErr
		}
		info, statErr := os.Stat(ed)
		if statErr == nil && info.ModTime().UnixNano() == idx.EntriesDirModTime {
			return append([]string(nil), idx.IDs...), nil
		}
		if statErr != nil && os.IsNotExist(statErr) && idx.EntriesDirModTime == 0 {
			return append([]string(nil), idx.IDs...), nil
		}
	}
	return rebuildEntryIndex()
}

func insertIndexedID(id string) error {
	ids, err := loadEntryIDs()
	if err != nil {
		return err
	}
	pos := sort.Search(len(ids), func(i int) bool { return ids[i] <= id })
	if pos < len(ids) && ids[pos] == id {
		return writeEntryIndex(ids)
	}
	ids = append(ids, "")
	copy(ids[pos+1:], ids[pos:])
	ids[pos] = id
	return writeEntryIndex(ids)
}

func removeIndexedID(id string) error {
	ids, err := loadEntryIDs()
	if err != nil {
		return err
	}
	for i, cur := range ids {
		if cur == id {
			ids = append(ids[:i], ids[i+1:]...)
			break
		}
	}
	return writeEntryIndex(ids)
}

func invalidateSummaryIndex() error {
	path, err := summaryIndexPath()
	if err != nil {
		return err
	}
	err = os.Remove(path)
	if err != nil && !os.IsNotExist(err) {
		return err
	}
	return nil
}

func writeSummaryIndex(ids []string) error {
	if err := Init(); err != nil {
		return err
	}
	path, err := summaryIndexPath()
	if err != nil {
		return err
	}
	tmpPath := path + ".tmp"
	f, err := os.OpenFile(tmpPath, os.O_CREATE|os.O_WRONLY|os.O_TRUNC, 0o600)
	if err != nil {
		return err
	}
	bw := bufio.NewWriter(f)
	header := summaryIndexHeader{Version: indexVersion}
	modTime, err := entriesDirModTime()
	if err != nil {
		f.Close()
		os.Remove(tmpPath)
		return err
	}
	header.EntriesDirModTime = modTime
	headerData, err := json.Marshal(header)
	if err != nil {
		f.Close()
		os.Remove(tmpPath)
		return err
	}
	if _, err := bw.Write(headerData); err != nil {
		f.Close()
		os.Remove(tmpPath)
		return err
	}
	if err := bw.WriteByte('\n'); err != nil {
		f.Close()
		os.Remove(tmpPath)
		return err
	}
	for _, id := range ids {
		m, err := GetMeta(id)
		if err != nil {
			var notFound *ErrNotFound
			if errors.As(err, &notFound) {
				continue
			}
			f.Close()
			os.Remove(tmpPath)
			return err
		}
		data, err := json.Marshal(m)
		if err != nil {
			f.Close()
			os.Remove(tmpPath)
			return err
		}
		if _, err := bw.Write(data); err != nil {
			f.Close()
			os.Remove(tmpPath)
			return err
		}
		if err := bw.WriteByte('\n'); err != nil {
			f.Close()
			os.Remove(tmpPath)
			return err
		}
	}
	if err := bw.Flush(); err != nil {
		f.Close()
		os.Remove(tmpPath)
		return err
	}
	if err := f.Close(); err != nil {
		os.Remove(tmpPath)
		return err
	}
	return os.Rename(tmpPath, path)
}

func streamSummaryIndex(yield summaryYieldFunc) error {
	path, err := summaryIndexPath()
	if err != nil {
		return err
	}
	f, err := os.Open(path)
	if err != nil {
		return err
	}
	defer f.Close()

	dec := json.NewDecoder(f)
	var header summaryIndexHeader
	if err := dec.Decode(&header); err != nil {
		return err
	}
	if header.Version != indexVersion {
		return fmt.Errorf("unsupported summary index version %d", header.Version)
	}
	modTime, err := entriesDirModTime()
	if err != nil {
		return err
	}
	if header.EntriesDirModTime != modTime {
		return fmt.Errorf("stale summary index")
	}

	for {
		var m Meta
		err := dec.Decode(&m)
		if err == io.EOF {
			break
		}
		if err != nil {
			return err
		}
		cont, err := yield(m)
		if err != nil {
			return err
		}
		if !cont {
			break
		}
	}
	return nil
}

// StreamSummaries iterates through the summary index in newest-first order.
// The callback can return false to stop iteration early.
func StreamSummaries(yield func(Meta) (bool, error)) error {
	err := streamSummaryIndex(yield)
	if err == nil {
		return nil
	}
	if _, rebuildErr := UpdateIndex(); rebuildErr != nil {
		return rebuildErr
	}
	return streamSummaryIndex(yield)
}

// UpdateIndex rebuilds the entry ID index from the current entries directory.
func UpdateIndex() (int, error) {
	ids, err := rebuildEntryIndex()
	if err != nil {
		return 0, err
	}
	if err := writeSummaryIndex(ids); err != nil {
		return 0, err
	}
	return len(ids), nil
}

// List returns all entries sorted newest first.
func List() ([]Meta, error) {
	metas := make([]Meta, 0)
	if err := StreamSummaries(func(m Meta) (bool, error) {
		metas = append(metas, m)
		return true, nil
	}); err != nil {
		return nil, err
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
	return invalidateSummaryIndex()
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
	ids, err := loadEntryIDs()
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
	ids, err := loadEntryIDs()
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

	ids, err := loadEntryIDs()
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

// Preview reads up to n bytes from the entry and returns them as a string,
// replacing non-printable bytes with '.'.
func Preview(id string, n int) (string, error) {
	ed, err := entriesDir()
	if err != nil {
		return "", err
	}
	f, err := os.Open(filepath.Join(ed, id, "data"))
	if err != nil {
		return "", err
	}
	defer f.Close()

	buf := make([]byte, n)
	nr, err := io.ReadFull(f, buf)
	if err != nil && err != io.ErrUnexpectedEOF && err != io.EOF {
		return "", err
	}
	buf = buf[:nr]

	for i, b := range buf {
		if b < 0x20 || b > 0x7e {
			buf[i] = '.'
		}
	}
	return string(buf), nil
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
	if err := removeIndexedID(id); err != nil {
		return err
	}
	return invalidateSummaryIndex()
}

// OlderThanIDs returns IDs older than the referenced canonical entry ID.
func OlderThanIDs(id string) ([]string, error) {
	ids, err := loadEntryIDs()
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
