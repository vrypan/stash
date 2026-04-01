package cmd

import (
	"encoding/json"
	"fmt"
	"os"
	"sort"
	"strings"
	"text/template"
	"time"
	"unicode/utf8"

	"stash/store"

	"github.com/fatih/color"
	"github.com/mattn/go-isatty"
	"github.com/spf13/cobra"
	"golang.org/x/sys/unix"
)

var (
	clrID    = color.New(color.FgYellow, color.Bold).SprintFunc()
	clrHash  = color.New(color.Faint).SprintFunc()
	clrLabel = color.New(color.Bold).SprintFunc()
	clrAttrs = color.New(color.FgMagenta).SprintFunc()
	clrFile  = color.New(color.FgCyan, color.Bold).SprintFunc()
)

func typeColor(t string) color.Attribute {
	switch t {
	case "text":
		return color.FgGreen
	case "json":
		return color.FgCyan
	case "empty":
		return color.Faint
	default: // binary, gzip, zstd, zip, png, jpeg, pdf, gif, …
		return color.FgCyan
	}
}

// clrTypeBare returns typeStr with color, no brackets — for long format.
func clrTypeBare(t string) string {
	return color.New(typeColor(t)).Sprint(t)
}

func displayTypeLabel(t string) string {
	base, _, _ := strings.Cut(t, ";")
	return strings.TrimSpace(base)
}

func longTypeLabel(t string) string {
	switch {
	case t == "text", t == "json", t == "application/json", strings.HasPrefix(t, "text/"):
		return t
	default:
		return clrTypeBare(t)
	}
}

func newLogCmd() *cobra.Command {
	var chars int
	var idMode string
	var metaFilters []string
	var n int
	var reverse bool
	var jsonFlag bool
	var formatStr string
	var dateMode string
	var noColor bool

	cmd := &cobra.Command{
		Use:           "log",
		Aliases:       []string{"list"},
		Short:         "Show detailed entry history",
		SilenceUsage:  true,
		SilenceErrors: true,
		RunE: func(c *cobra.Command, _ []string) error {
			if noColor {
				color.NoColor = true
			}
			if dateMode != "absolute" && dateMode != "relative" {
				return fmt.Errorf("--date must be absolute or relative")
			}
			if idMode != "short" && idMode != "full" && idMode != "pos" {
				return fmt.Errorf("--id must be short, full, or pos")
			}
			effectiveDateMode := dateMode
			effectiveIDMode := idMode
			effectiveChars := chars

			entries, err := collectEntries(metaFilters, reverse, n)
			if err != nil {
				return err
			}

			now := time.Now()
			if !c.Flags().Changed("chars") && formatStr == "" && !jsonFlag {
				if width, ok := terminalWidth(); ok {
					effectiveChars = width - 4
					if effectiveChars < 20 {
						effectiveChars = 20
					}
				}
			}

			if formatStr != "" {
				return logTemplate(entries, now, effectiveChars, effectiveDateMode, formatStr)
			}
			if jsonFlag {
				return logJSON(entries, now, effectiveChars, effectiveDateMode)
			}
			return logLong(entries, now, effectiveChars, effectiveDateMode, effectiveIDMode)
		},
	}

	cmd.Flags().IntVar(&chars, "chars", 80, "Preview character limit")
	cmd.Flags().StringVar(&idMode, "id", "full", "ID display: short, full, or pos")
	cmd.Flags().StringArrayVar(&metaFilters, "meta", nil, "Filter by metadata key or key=value (repeatable)")
	cmd.Flags().IntVarP(&n, "number", "n", 0, "Limit number of entries shown (0 = all)")
	cmd.Flags().BoolVar(&reverse, "reverse", false, "Show oldest first")
	cmd.Flags().BoolVar(&jsonFlag, "json", false, "Output verbose entry history as JSON")
	cmd.Flags().StringVar(&formatStr, "format", "", "Go template for custom log output")
	cmd.Flags().StringVar(&dateMode, "date", "absolute", "Date format: relative or absolute")
	cmd.Flags().BoolVar(&noColor, "no-color", false, "Disable color output")
	return cmd
}

type metaFilter struct {
	key   string
	value string
	exact bool
}

func parseMetaFilters(inputs []string) ([]metaFilter, error) {
	filters := make([]metaFilter, 0, len(inputs))
	for _, input := range inputs {
		input = strings.TrimSpace(input)
		if input == "" {
			return nil, fmt.Errorf("invalid --meta filter %q", input)
		}
		key, value, exact := strings.Cut(input, "=")
		key = strings.TrimSpace(key)
		if key == "" {
			return nil, fmt.Errorf("invalid --meta filter %q", input)
		}
		filters = append(filters, metaFilter{
			key:   key,
			value: value,
			exact: exact,
		})
	}
	return filters, nil
}

func matchesMetaFilters(attrs map[string]string, filters []metaFilter) bool {
	for _, f := range filters {
		v, ok := attrs[f.key]
		if !ok {
			return false
		}
		if f.exact && v != f.value {
			return false
		}
	}
	return true
}

func collectEntries(inputs []string, reverse bool, n int) ([]store.Summary, error) {
	filters, err := parseMetaFilters(inputs)
	if err != nil {
		return nil, err
	}
	out := make([]store.Summary, 0)
	if err := store.StreamSummaries(func(s store.Summary) (bool, error) {
		if !matchesMetaFilters(s.Attrs, filters) {
			return true, nil
		}
		out = append(out, s)
		if !reverse && n > 0 && len(out) >= n {
			return false, nil
		}
		return true, nil
	}); err != nil {
		return nil, err
	}
	if reverse {
		for i, j := 0, len(out)-1; i < j; i, j = i+1, j-1 {
			out[i], out[j] = out[j], out[i]
		}
	}
	if n > 0 && len(out) > n {
		out = out[:n]
	}
	return out, nil
}

func autoPreviewChars(entries []store.Summary, now time.Time, idMode string, dateMode string) int {
	width, ok := terminalWidth()
	if !ok {
		return 80
	}

	maxID, maxTS, maxSize := 0, 0, 0
	for i, m := range entries {
		idStr := m.ShortID()
		switch idMode {
		case "full":
			idStr = m.DisplayID()
		case "pos":
			idStr = fmt.Sprintf("%d", i+1)
		}
		if len(idStr) > maxID {
			maxID = len(idStr)
		}
		tsStr := formatTS(parseTS(m.TS), now, dateMode)
		if len(tsStr) > maxTS {
			maxTS = len(tsStr)
		}
		sizeStr := store.HumanSize(m.Size)
		if len(sizeStr) > maxSize {
			maxSize = len(sizeStr)
		}
	}

	fixed := maxID + maxTS + maxSize + 6
	chars := width - fixed
	if chars < 20 {
		return 20
	}
	return chars
}

func terminalWidth() (int, bool) {
	fd := os.Stdout.Fd()
	if !isatty.IsTerminal(fd) {
		return 0, false
	}
	ws, err := unix.IoctlGetWinsize(int(fd), unix.TIOCGWINSZ)
	if err != nil || ws == nil || ws.Col == 0 {
		return 0, false
	}
	return int(ws.Col), true
}

func trimANSIToWidth(s string, width int) string {
	if width <= 0 {
		return ""
	}
	var b strings.Builder
	visible := 0
	for i := 0; i < len(s); {
		if s[i] == 0x1b && i+1 < len(s) && s[i+1] == '[' {
			j := i + 2
			for j < len(s) {
				c := s[j]
				if c >= 0x40 && c <= 0x7e {
					j++
					break
				}
				j++
			}
			b.WriteString(s[i:j])
			i = j
			continue
		}
		r, size := utf8.DecodeRuneInString(s[i:])
		if r == utf8.RuneError && size == 1 {
			size = 1
		}
		if visible >= width {
			break
		}
		b.WriteString(s[i : i+size])
		visible++
		i += size
	}
	if visible >= width {
		b.WriteString("\x1b[0m")
	}
	return b.String()
}

// fmtAttrs formats a meta map as "[key=val  key=val]", sorted by key.
// Returns an empty string when attrs is nil or empty.
func fmtAttrs(attrs map[string]string) string {
	if len(attrs) == 0 {
		return ""
	}
	keys := make([]string, 0, len(attrs))
	for k := range attrs {
		keys = append(keys, k)
	}
	sort.Strings(keys)
	pairs := make([]string, len(keys))
	for i, k := range keys {
		pairs[i] = k + "=" + attrs[k]
	}
	return "[" + strings.Join(pairs, "  ") + "]"
}

type logJSONEntry struct {
	ID        string            `json:"id"`
	ShortID   string            `json:"short_id"`
	StackRef  string            `json:"stack_ref"`
	TS        string            `json:"ts"`
	Date      string            `json:"date"`
	Hash      string            `json:"hash"`
	Size      int64             `json:"size"`
	SizeHuman string            `json:"size_human"`
	Type      string            `json:"type,omitempty"`
	MIME      string            `json:"mime,omitempty"`
	Meta      map[string]string `json:"meta,omitempty"`
	Preview   []string          `json:"preview,omitempty"`
}

func buildLogJSONEntry(s store.Summary, idx int, now time.Time, chars int, dateMode string) logJSONEntry {
	var lines []string
	if p := strings.TrimSpace(s.Preview); p != "" {
		lines = []string{p}
	}
	return logJSONEntry{
		ID:        s.DisplayID(),
		ShortID:   s.ShortID(),
		StackRef:  fmt.Sprintf("%d", idx+1),
		TS:        s.TS,
		Date:      formatTS(parseTS(s.TS), now, dateMode),
		Hash:      s.Hash,
		Size:      s.Size,
		SizeHuman: store.HumanSize(s.Size),
		Type:      s.Type,
		MIME:      s.MIME,
		Meta:      s.Attrs,
		Preview:   lines,
	}
}

func logJSON(entries []store.Summary, now time.Time, chars int, dateMode string) error {
	out := make([]logJSONEntry, len(entries))
	for i, s := range entries {
		out[i] = buildLogJSONEntry(s, i, now, chars, dateMode)
	}
	enc := json.NewEncoder(color.Output)
	enc.SetIndent("", "  ")
	return enc.Encode(out)
}

func logTemplate(entries []store.Summary, now time.Time, chars int, dateMode, formatStr string) error {
	tmpl, err := template.New("log").Parse(formatStr)
	if err != nil {
		return fmt.Errorf("invalid --format template: %w", err)
	}
	for i, s := range entries {
		item := buildLogJSONEntry(s, i, now, chars, dateMode)
		item.MIME = displayTypeLabel(item.MIME)
		if err := tmpl.Execute(color.Output, item); err != nil {
			return fmt.Errorf("render --format template: %w", err)
		}
		fmt.Fprintln(color.Output)
	}
	return nil
}

func logLong(entries []store.Summary, now time.Time, chars int, dateMode, idMode string) error {
	for i, s := range entries {
		if i > 0 {
			fmt.Println()
		}
		item := buildLogJSONEntry(s, i, now, chars, dateMode)
		tsStr := item.Date
		typeLabel := item.MIME
		if typeLabel == "" {
			typeLabel = item.Type
		}
		typeLabel = displayTypeLabel(typeLabel)

		idLabel := item.ShortID
		switch idMode {
		case "full":
			idLabel = item.ID
		case "pos":
			idLabel = item.StackRef
		}
		fmt.Printf("entry %s (%s, %s)\n", clrID(idLabel), longTypeLabel(typeLabel), item.SizeHuman)
		fmt.Printf("%s%s\n", clrLabel("Date: "), tsStr)
		fmt.Printf("%s%s\n", clrLabel("Hash: "), clrHash(item.Hash))
		if a := fmtAttrs(item.Meta); a != "" {
			fmt.Printf("%s%s\n", clrLabel("Meta: "), clrAttrs(a))
		}

		lines := item.Preview
		if len(lines) > 0 {
			fmt.Printf("\n    %s\n", lines[0])
			for _, line := range lines[1:] {
				fmt.Printf("    %s\n", line)
			}
		}
	}
	return nil
}
