package cmd

import (
	"fmt"
	"strings"
	"time"

	"stash/store"

	"github.com/spf13/cobra"
)

func newLsCmd() *cobra.Command {
	var chars int
	var idMode string
	var metaFilters []string
	var mime bool
	var n int
	var preview bool
	var reverse bool
	var long bool
	var dateMode string

	cmd := &cobra.Command{
		Use:           "ls",
		Short:         "Show a filename-oriented view of stash entries",
		SilenceUsage:  true,
		SilenceErrors: true,
		RunE: func(c *cobra.Command, _ []string) error {
			if dateMode != "absolute" && dateMode != "relative" && dateMode != "ls" {
				return fmt.Errorf("--date must be ls, absolute, or relative")
			}
			if idMode != "short" && idMode != "full" && idMode != "pos" {
				return fmt.Errorf("--id must be short, full, or pos")
			}
			effectiveDateMode := dateMode
			if long && !c.Flags().Changed("date") {
				effectiveDateMode = "ls"
			}
			effectiveIDMode := idMode
			if long && !c.Flags().Changed("id") {
				effectiveIDMode = "full"
			}
			effectiveChars := chars

			entries, err := store.List()
			if err != nil {
				return err
			}
			filters, err := parseMetaFilters(metaFilters)
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
			if preview && !c.Flags().Changed("chars") {
				effectiveChars = autoPreviewChars(entries, now, effectiveIDMode, false, effectiveDateMode)
			}
			if long {
				return lsLong(entries, now, effectiveDateMode, effectiveIDMode, mime, preview, effectiveChars, filters)
			}
			return lsCompact(entries, now, effectiveDateMode, effectiveIDMode, mime, preview, effectiveChars, filters)
		},
	}

	cmd.Flags().IntVar(&chars, "chars", 80, "Preview character limit")
	cmd.Flags().StringVar(&idMode, "id", "short", "ID display: short, full, or pos")
	cmd.Flags().StringArrayVar(&metaFilters, "meta", nil, "Filter by metadata key or key=value (repeatable)")
	cmd.Flags().BoolVar(&mime, "mime", false, "Include MIME/type column")
	cmd.Flags().IntVarP(&n, "number", "n", 0, "Limit number of entries shown (0 = all)")
	cmd.Flags().BoolVarP(&preview, "preview", "p", false, "Append compact preview text")
	cmd.Flags().BoolVar(&reverse, "reverse", false, "Show oldest first")
	cmd.Flags().BoolVarP(&long, "long", "l", false, "Verbose file-oriented listing")
	cmd.Flags().StringVar(&dateMode, "date", "relative", "Date format: ls, relative, or absolute")
	return cmd
}

func lsDate(t, now time.Time) string {
	if t.Year() == now.Year() {
		return t.Local().Format("Jan _2 15:04")
	}
	return t.Local().Format("Jan _2  2006")
}

func lsID(m store.Meta, idx int, idMode string) string {
	switch idMode {
	case "full":
		return m.DisplayID()
	case "pos":
		return fmt.Sprintf("%d", idx+1)
	default:
		return m.ShortID()
	}
}

func lsName(m store.Meta) string {
	if name := strings.TrimSpace(m.Attrs["filename"]); name != "" {
		return name
	}
	return m.DisplayID()
}

func lsMatchedAttrs(attrs map[string]string, filters []metaFilter) string {
	if len(filters) == 0 || len(attrs) == 0 {
		return ""
	}
	parts := make([]string, 0, len(filters))
	seen := make(map[string]bool, len(filters))
	for _, f := range filters {
		if seen[f.key] {
			continue
		}
		v, ok := attrs[f.key]
		if !ok {
			continue
		}
		if f.exact {
			parts = append(parts, f.key+"="+v)
		} else {
			parts = append(parts, v)
		}
		seen[f.key] = true
	}
	if len(parts) == 0 {
		return ""
	}
	return "[" + strings.Join(parts, "  ") + "]"
}

func lsCompact(entries []store.Meta, now time.Time, dateMode, idMode string, mime, preview bool, chars int, filters []metaFilter) error {
	type row struct {
		id, name, matched, size, date, mime, preview string
	}
	rows := make([]row, len(entries))
	maxID, maxName, maxSize, maxDate := 0, 0, 0, 0

	for i, m := range entries {
		r := row{
			id:      lsID(m, i, idMode),
			name:    lsName(m),
			matched: lsMatchedAttrs(m.Attrs, filters),
			size:    store.HumanSize(m.Size),
			date:    formatTS(parseTS(m.TS), now, dateMode),
			mime:    displayTypeLabel(m.MIME),
		}
		if r.mime == "" {
			r.mime = m.Type
		}
		if preview {
			typeStr, p, _ := store.SmartPreview(m.ID, chars)
			if typeStr == "text" || typeStr == "json" {
				if m.Size > int64(chars) {
					p += "..."
				}
				r.preview = p
			}
		}
		rows[i] = r
		if len(r.id) > maxID {
			maxID = len(r.id)
		}
		if len(r.name) > maxName {
			maxName = len(r.name)
		}
		if len(r.size) > maxSize {
			maxSize = len(r.size)
		}
		if len(r.date) > maxDate {
			maxDate = len(r.date)
		}
	}

	for _, r := range rows {
		idCol := clrID(fmt.Sprintf("%-*s", maxID, r.id))
		nameCol := r.name
		if nameCol != r.id {
			nameCol = clrFile(fmt.Sprintf("%-*s", maxName, r.name))
		} else {
			nameCol = fmt.Sprintf("%-*s", maxName, r.name)
		}
		line := fmt.Sprintf("%s  %*s  %*s  %s", idCol, maxSize, r.size, maxDate, r.date, nameCol)
		if r.matched != "" {
			line += "  " + clrAttrs(r.matched)
		}
		if mime && r.mime != "" {
			line += "  " + r.mime
		}
		if r.preview != "" {
			line += "  " + r.preview
		}
		if width, ok := terminalWidth(); ok {
			line = trimANSIToWidth(line, width)
		}
		fmt.Println(line)
	}
	return nil
}

func lsLong(entries []store.Meta, now time.Time, dateMode, idMode string, mime, preview bool, chars int, filters []metaFilter) error {
	type row struct {
		id, name, matched, size, date, mime, preview string
	}
	rows := make([]row, len(entries))
	maxID, maxName, maxSize, maxDate := 0, 0, 0, 0

	for i, m := range entries {
		mime := displayTypeLabel(m.MIME)
		if mime == "" {
			mime = m.Type
		}
		r := row{
			id:      lsID(m, i, idMode),
			name:    lsName(m),
			matched: lsMatchedAttrs(m.Attrs, filters),
			size:    store.HumanSize(m.Size),
			date:    lsDate(parseTS(m.TS), now),
			mime:    mime,
		}
		if preview {
			typeStr, p, _ := store.SmartPreview(m.ID, chars)
			if typeStr == "text" || typeStr == "json" {
				if m.Size > int64(chars) {
					p += "..."
				}
				r.preview = p
			}
		}
		rows[i] = r
		if len(r.id) > maxID {
			maxID = len(r.id)
		}
		if len(r.name) > maxName {
			maxName = len(r.name)
		}
		if len(r.size) > maxSize {
			maxSize = len(r.size)
		}
		if len(r.date) > maxDate {
			maxDate = len(r.date)
		}
	}

	for _, r := range rows {
		idCol := clrID(fmt.Sprintf("%-*s", maxID, r.id))
		nameCol := r.name
		if nameCol != r.id {
			nameCol = clrFile(fmt.Sprintf("%-*s", maxName, r.name))
		} else {
			nameCol = fmt.Sprintf("%-*s", maxName, r.name)
		}
		line := fmt.Sprintf("%s  %*s  %*s  %s", idCol, maxSize, r.size, maxDate, r.date, nameCol)
		if r.matched != "" {
			line += "  " + clrAttrs(r.matched)
		}
		if mime && r.mime != "" {
			line += "  " + r.mime
		}
		if r.preview != "" {
			line += "  " + r.preview
		}
		if width, ok := terminalWidth(); ok {
			line = trimANSIToWidth(line, width)
		}
		fmt.Println(line)
	}
	return nil
}
