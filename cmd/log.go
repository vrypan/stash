package cmd

import (
	"encoding/json"
	"fmt"
	"sort"
	"strings"
	"text/template"
	"time"

	"stash/store"

	"github.com/fatih/color"
	"github.com/spf13/cobra"
)

var (
	clrID    = color.New(color.FgYellow, color.Bold).SprintFunc()
	clrTS    = color.New(color.FgCyan).SprintFunc()
	clrSize  = color.New(color.FgBlue).SprintFunc()
	clrHash  = color.New(color.Faint).SprintFunc()
	clrLabel = color.New(color.Bold).SprintFunc()
	clrAttrs = color.New(color.FgYellow).SprintFunc()
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
		return color.FgRed
	}
}

// clrType returns "[typeStr]" with color — for compact format.
func clrType(t string) string {
	return color.New(typeColor(t)).Sprint("[" + t + "]")
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
	var hashFlag bool
	var idMode string
	var metaFilters []string
	var n int
	var reverse bool
	var long bool
	var jsonFlag bool
	var formatStr string
	var dateMode string
	var noColor bool

	cmd := &cobra.Command{
		Use:           "log",
		Aliases:       []string{"list"},
		Short:         "Show entry history with content preview",
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
			if long && !c.Flags().Changed("date") {
				effectiveDateMode = "absolute"
			}
			effectiveIDMode := idMode
			if long && !c.Flags().Changed("id") {
				effectiveIDMode = "full"
			}

			entries, err := store.List()
			if err != nil {
				return err
			}
			entries, err = filterEntriesByMeta(entries, metaFilters)
			if err != nil {
				return err
			}

			if reverse {
				for i, j := 0, len(entries)-1; i < j; i, j = i+1, j-1 {
					entries[i], entries[j] = entries[j], entries[i]
				}
			}
			if n > 0 && len(entries) > n {
				entries = entries[:n]
			}

			now := time.Now()

			if formatStr != "" {
				return logTemplate(entries, now, chars, effectiveDateMode, formatStr)
			}
			if jsonFlag {
				return logJSON(entries, now, chars, effectiveDateMode)
			}
			if long {
				return logLong(entries, now, chars, effectiveDateMode, effectiveIDMode)
			}
			return logCompact(entries, now, chars, effectiveIDMode, hashFlag, effectiveDateMode)
		},
	}

	cmd.Flags().IntVar(&chars, "chars", 80, "Preview character limit")
	cmd.Flags().BoolVar(&hashFlag, "hash", false, "Include hash")
	cmd.Flags().StringVar(&idMode, "id", "short", "ID display: short, full, or pos")
	cmd.Flags().StringArrayVar(&metaFilters, "meta", nil, "Filter by metadata key or key=value (repeatable)")
	cmd.Flags().IntVarP(&n, "number", "n", 0, "Limit number of entries shown (0 = all)")
	cmd.Flags().BoolVar(&reverse, "reverse", false, "Show oldest first")
	cmd.Flags().BoolVarP(&long, "long", "l", false, "Verbose block format")
	cmd.Flags().BoolVar(&jsonFlag, "json", false, "Output verbose entry history as JSON")
	cmd.Flags().StringVar(&formatStr, "format", "", "Go template for custom log output")
	cmd.Flags().StringVar(&dateMode, "date", "relative", "Date format: relative or absolute")
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

func filterEntriesByMeta(entries []store.Meta, inputs []string) ([]store.Meta, error) {
	if len(inputs) == 0 {
		return entries, nil
	}
	filters, err := parseMetaFilters(inputs)
	if err != nil {
		return nil, err
	}
	out := make([]store.Meta, 0, len(entries))
	for _, m := range entries {
		if matchesMetaFilters(m.Attrs, filters) {
			out = append(out, m)
		}
	}
	return out, nil
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

func logCompact(entries []store.Meta, now time.Time, chars int, idMode string, hash bool, dateMode string) error {
	type row struct {
		id, ts, size, hash, typeStr, typeLabel, preview string
		truncated                                       bool
	}
	rows := make([]row, len(entries))
	maxID, maxTS, maxSize, maxHash := 0, 0, 0, 0

	for i, m := range entries {
		idStr := m.ShortID()
		switch idMode {
		case "full":
			idStr = m.DisplayID()
		case "pos":
			idStr = fmt.Sprintf("%d", i+1)
		}
		tsStr := formatTS(parseTS(m.TS), now, dateMode)
		typeStr, preview, _ := store.SmartPreview(m.ID, chars)
		typeLabel := m.MIME
		if typeLabel == "" {
			typeLabel = m.Type
		}
		if typeLabel == "" {
			typeLabel = typeStr
		}
		typeLabel = displayTypeLabel(typeLabel)
		sizeStr := store.HumanSize(m.Size)

		rows[i] = row{
			id:        idStr,
			ts:        tsStr,
			size:      sizeStr,
			hash:      m.Hash,
			typeStr:   typeStr,
			typeLabel: typeLabel,
			preview:   preview,
			truncated: (typeStr == "text" || typeStr == "json") && m.Size > int64(chars),
		}
		if len(idStr) > maxID {
			maxID = len(idStr)
		}
		if len(tsStr) > maxTS {
			maxTS = len(tsStr)
		}
		if len(sizeStr) > maxSize {
			maxSize = len(sizeStr)
		}
		if hash && len(m.Hash) > maxHash {
			maxHash = len(m.Hash)
		}
	}

	for _, r := range rows {
		// Pad plain strings first, then colorize — ANSI codes add invisible
		// bytes that would confuse any width-based padding done afterwards.
		idCol := clrID(fmt.Sprintf("%-*s", maxID, r.id))
		tsCol := clrTS(fmt.Sprintf("%-*s", maxTS, r.ts))
		sizeCol := clrSize(fmt.Sprintf("%-*s", maxSize, r.size))

		var parts []string
		if r.typeStr != "text" && r.typeStr != "json" && r.typeStr != "empty" {
			parts = append(parts, clrType(r.typeLabel))
		}
		if (r.typeStr == "text" || r.typeStr == "json") && r.preview != "" {
			p := r.preview
			if r.truncated {
				p += "..."
			}
			parts = append(parts, p)
		}
		contentCol := strings.Join(parts, " ")

		if hash {
			hashCol := clrHash(fmt.Sprintf("%-*s", maxHash, r.hash))
			fmt.Printf("%s  %s  %s  %s  %s\n", idCol, tsCol, sizeCol, hashCol, contentCol)
		} else {
			fmt.Printf("%s  %s  %s  %s\n", idCol, tsCol, sizeCol, contentCol)
		}
	}
	return nil
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

func buildLogJSONEntry(m store.Meta, idx int, now time.Time, chars int, dateMode string) logJSONEntry {
	lines, _ := store.LongPreview(m.ID, chars, 5)
	for len(lines) > 0 && lines[len(lines)-1] == "" {
		lines = lines[:len(lines)-1]
	}
	return logJSONEntry{
		ID:        m.DisplayID(),
		ShortID:   m.ShortID(),
		StackRef:  fmt.Sprintf("%d", idx+1),
		TS:        m.TS,
		Date:      formatTS(parseTS(m.TS), now, dateMode),
		Hash:      m.Hash,
		Size:      m.Size,
		SizeHuman: store.HumanSize(m.Size),
		Type:      m.Type,
		MIME:      m.MIME,
		Meta:      m.Attrs,
		Preview:   lines,
	}
}

func logJSON(entries []store.Meta, now time.Time, chars int, dateMode string) error {
	out := make([]logJSONEntry, len(entries))
	for i, m := range entries {
		out[i] = buildLogJSONEntry(m, i, now, chars, dateMode)
	}
	enc := json.NewEncoder(color.Output)
	enc.SetIndent("", "  ")
	return enc.Encode(out)
}

func logTemplate(entries []store.Meta, now time.Time, chars int, dateMode, formatStr string) error {
	tmpl, err := template.New("log").Parse(formatStr)
	if err != nil {
		return fmt.Errorf("invalid --format template: %w", err)
	}
	for i, m := range entries {
		item := buildLogJSONEntry(m, i, now, chars, dateMode)
		item.MIME = displayTypeLabel(item.MIME)
		if err := tmpl.Execute(color.Output, item); err != nil {
			return fmt.Errorf("render --format template: %w", err)
		}
		fmt.Fprintln(color.Output)
	}
	return nil
}

func logLong(entries []store.Meta, now time.Time, chars int, dateMode, idMode string) error {
	for i, m := range entries {
		if i > 0 {
			fmt.Println()
		}
		item := buildLogJSONEntry(m, i, now, chars, dateMode)
		tsStr := item.Date
		typeLabel := item.MIME
		if typeLabel == "" {
			typeLabel = item.Type
		}
		if typeLabel == "" {
			detectedType, _, _ := store.SmartPreview(m.ID, chars)
			typeLabel = detectedType
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
