package cmd

import (
	"fmt"
	"time"
)

func parseTS(s string) time.Time {
	t, err := time.Parse(time.RFC3339Nano, s)
	if err != nil {
		t, _ = time.Parse(time.RFC3339, s)
	}
	return t
}

const isoFormat = "Mon Jan _2 15:04:05 2006 -0700"

func normalizeDateMode(mode string) string {
	switch mode {
	case "absolute":
		return "iso"
	case "relative":
		return "ago"
	default:
		return mode
	}
}

func formatTS(t time.Time, now time.Time, mode string) string {
	switch normalizeDateMode(mode) {
	case "iso":
		return t.Local().Format(isoFormat)
	case "ago":
		return relativeTime(now, t)
	default:
		return t.Local().Format(isoFormat)
	}
}

func formatLSDate(t, now time.Time, mode string) string {
	switch normalizeDateMode(mode) {
	case "iso":
		return formatTS(t, now, "iso")
	case "ago":
		return formatTS(t, now, "ago")
	default:
		return lsDate(t, now)
	}
}

func relativeTime(now, t time.Time) string {
	d := now.Sub(t)
	if d < 0 {
		d = 0
	}
	switch {
	case d < time.Minute:
		return fmt.Sprintf("%ds ago", int(d.Seconds()))
	case d < time.Hour:
		return fmt.Sprintf("%dm ago", int(d.Minutes()))
	case d < 24*time.Hour:
		return fmt.Sprintf("%dh ago", int(d.Hours()))
	case d < 30*24*time.Hour:
		return fmt.Sprintf("%dd ago", int(d.Hours()/24))
	case d < 365*24*time.Hour:
		return fmt.Sprintf("%dmo ago", int(d.Hours()/(24*30)))
	default:
		return fmt.Sprintf("%dy ago", int(d.Hours()/(24*365)))
	}
}
