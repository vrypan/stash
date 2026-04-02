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
	var dateMode string
	var sizeMode string
	var name bool
	var typeCol bool
	var subtypeCol bool
	var n int
	var preview bool
	var reverse bool
	var long bool

	cmd := &cobra.Command{
		Use:           "ls",
		Short:         "List stash entries",
		SilenceUsage:  true,
		SilenceErrors: true,
		RunE: func(c *cobra.Command, _ []string) error {
			if dateMode != "" && dateMode != "absolute" && dateMode != "relative" && dateMode != "ls" {
				return fmt.Errorf("--date must be absolute, relative, or ls")
			}
			if sizeMode != "" && sizeMode != "human" && sizeMode != "bytes" {
				return fmt.Errorf("--size must be human or bytes")
			}
			if idMode != "short" && idMode != "full" && idMode != "pos" {
				return fmt.Errorf("--id must be short, full, or pos")
			}
			effectiveDateMode := dateMode
			effectiveSizeMode := sizeMode
			effectiveName := name
			if long {
				if !c.Flags().Changed("date") {
					effectiveDateMode = "absolute"
				}
				if !c.Flags().Changed("size") {
					effectiveSizeMode = "human"
				}
				if !c.Flags().Changed("name") {
					effectiveName = true
				}
			}
			effectiveChars := chars

			filters, err := parseMetaFilters(metaFilters)
			if err != nil {
				return err
			}
			entries, err := collectEntries(metaFilters, reverse, n)
			if err != nil {
				return err
			}

			now := time.Now()
			if preview && !c.Flags().Changed("chars") {
				effectiveChars = autoLSPreviewChars(entries, now, idMode, effectiveDateMode, effectiveSizeMode, effectiveName, typeCol, subtypeCol)
			}
			return renderLS(entries, now, idMode, effectiveDateMode, effectiveSizeMode, effectiveName, typeCol, subtypeCol, preview, effectiveChars, filters)
		},
	}

	cmd.Flags().IntVar(&chars, "chars", 80, "Preview character limit")
	cmd.Flags().StringVar(&idMode, "id", "short", "ID display: short, full, or pos")
	cmd.Flags().StringArrayVar(&metaFilters, "meta", nil, "Filter by metadata key or key=value (repeatable)")
	cmd.Flags().StringVar(&dateMode, "date", "", "Include date column: absolute, relative, or ls")
	cmd.Flags().Lookup("date").NoOptDefVal = "absolute"
	cmd.Flags().StringVar(&sizeMode, "size", "", "Include size column: human or bytes")
	cmd.Flags().Lookup("size").NoOptDefVal = "human"
	cmd.Flags().BoolVar(&name, "name", false, "Include filename or full ULID column")
	cmd.Flags().BoolVar(&typeCol, "type", false, "Include MIME type column")
	cmd.Flags().BoolVar(&subtypeCol, "subtype", false, "Include MIME subtype column")
	cmd.Flags().IntVarP(&n, "number", "n", 0, "Limit number of entries shown (0 = all)")
	cmd.Flags().BoolVarP(&preview, "preview", "p", false, "Append compact preview text")
	cmd.Flags().BoolVar(&reverse, "reverse", false, "Show oldest first")
	cmd.Flags().BoolVarP(&long, "long", "l", false, "Alias for --date --size --name")
	return cmd
}

func lsDate(t, now time.Time) string {
	if t.Year() == now.Year() {
		return t.Local().Format("Jan _2 15:04")
	}
	return t.Local().Format("Jan _2  2006")
}

func lsID(m store.Summary, idx int, idMode string) string {
	switch idMode {
	case "full":
		return m.DisplayID()
	case "pos":
		return fmt.Sprintf("%d", idx+1)
	default:
		return m.ShortID()
	}
}

func lsName(m store.Summary) string {
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

type lsRow struct {
	id, name, matched, size, date, typ, subtype, preview string
}

func formatLSSize(size int64, mode string) string {
	if mode == "bytes" {
		return fmt.Sprintf("%d", size)
	}
	return store.HumanSize(size)
}

func autoLSPreviewChars(entries []store.Summary, now time.Time, idMode, dateMode, sizeMode string, name, typeCol, subtypeCol bool) int {
	width, ok := terminalWidth()
	if !ok {
		return 80
	}
	maxID, maxSize, maxDate, maxName, maxType, maxSubtype := 0, 0, 0, 0, 0, 0
	for i, m := range entries {
		if id := lsID(m, i, idMode); len(id) > maxID {
			maxID = len(id)
		}
		if sizeMode != "" {
			if size := formatLSSize(m.Size, sizeMode); len(size) > maxSize {
				maxSize = len(size)
			}
		}
		if dateMode != "" {
			if date := formatLSDate(parseTS(m.TS), now, dateMode); len(date) > maxDate {
				maxDate = len(date)
			}
		}
		if name {
			if n := lsName(m); len(n) > maxName {
				maxName = len(n)
			}
		}
		if typeCol {
			if label := m.MIMEMajor(); len(label) > maxType {
				maxType = len(label)
			}
		}
		if subtypeCol {
			if label := m.MIMESubtype(); len(label) > maxSubtype {
				maxSubtype = len(label)
			}
		}
	}
	fixed := maxID
	if sizeMode != "" {
		fixed += 2 + maxSize
	}
	if dateMode != "" {
		fixed += 2 + maxDate
	}
	if name {
		fixed += 2 + maxName
	}
	if typeCol {
		fixed += 2 + maxType
	}
	if subtypeCol {
		fixed += 2 + maxSubtype
	}
	chars := width - fixed - 2
	if chars < 20 {
		return 20
	}
	return chars
}

func buildLSRows(entries []store.Summary, now time.Time, dateMode, sizeMode, idMode string, name, preview bool, chars int, filters []metaFilter) []lsRow {
	rows := make([]lsRow, len(entries))
	for i, m := range entries {
		r := lsRow{
			id: lsID(m, i, idMode),
		}
		if name {
			r.name = lsName(m)
			r.matched = lsMatchedAttrs(m.Attrs, filters)
		}
		if sizeMode != "" {
			r.size = formatLSSize(m.Size, sizeMode)
		}
		if dateMode != "" {
			r.date = formatLSDate(parseTS(m.TS), now, dateMode)
		}
		r.typ = m.MIMEMajor()
		r.subtype = m.MIMESubtype()
		if preview && m.Preview != "" {
			r.preview = m.Preview
			if m.Size > int64(chars) {
				r.preview += "..."
			}
		}
		rows[i] = r
	}
	return rows
}

func renderLS(entries []store.Summary, now time.Time, idMode, dateMode, sizeMode string, name, typeCol, subtypeCol, preview bool, chars int, filters []metaFilter) error {
	if dateMode == "" && sizeMode == "" && !name && !typeCol && !subtypeCol && !preview {
		for i, m := range entries {
			fmt.Println(lsID(m, i, idMode))
		}
		return nil
	}

	rows := buildLSRows(entries, now, dateMode, sizeMode, idMode, name, preview, chars, filters)
	if dateMode == "ls" {
		for i, m := range entries {
			rows[i].date = lsDate(parseTS(m.TS), now)
		}
	}

	maxID, maxName, maxSize, maxDate, maxType, maxSubtype := 0, 0, 0, 0, 0, 0
	for _, r := range rows {
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
		if len(r.typ) > maxType {
			maxType = len(r.typ)
		}
		if len(r.subtype) > maxSubtype {
			maxSubtype = len(r.subtype)
		}
	}

	for _, r := range rows {
		idCol := clrID(fmt.Sprintf("%-*s", maxID, r.id))
		parts := []string{idCol}
		if r.size != "" {
			parts = append(parts, fmt.Sprintf("%*s", maxSize, r.size))
		}
		if r.date != "" {
			parts = append(parts, fmt.Sprintf("%*s", maxDate, r.date))
		}
		if r.name != "" {
			nameCol := r.name
			if nameCol != r.id {
				nameCol = clrFile(fmt.Sprintf("%-*s", maxName, r.name))
			} else {
				nameCol = fmt.Sprintf("%-*s", maxName, r.name)
			}
			parts = append(parts, nameCol)
			if r.matched != "" {
				parts = append(parts, clrAttrs(r.matched))
			}
		}
		if typeCol && r.typ != "" {
			parts = append(parts, fmt.Sprintf("%-*s", maxType, r.typ))
		}
		if subtypeCol && r.subtype != "" {
			parts = append(parts, fmt.Sprintf("%-*s", maxSubtype, r.subtype))
		}
		if r.preview != "" {
			parts = append(parts, r.preview)
		}
		line := strings.Join(parts, "  ")
		if width, ok := terminalWidth(); ok {
			line = trimANSIToWidth(line, width)
		}
		fmt.Println(line)
	}
	return nil
}
