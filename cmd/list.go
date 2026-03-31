package cmd

import (
	"fmt"
	"os"
	"text/tabwriter"
	"time"

	"github.com/spf13/cobra"
	"stash/store"
)

func newListCmd() *cobra.Command {
	var fullFlag bool
	var previewChars int
	var dateMode string

	cmd := &cobra.Command{
		Use:           "list",
		Short:         "List entries, newest first",
		SilenceUsage:  true,
		SilenceErrors: true,
		RunE: func(_ *cobra.Command, _ []string) error {
			if dateMode != "absolute" && dateMode != "relative" {
				return fmt.Errorf("--date must be absolute or relative")
			}

			entries, err := store.List()
			if err != nil {
				return err
			}

			now := time.Now()
			w := tabwriter.NewWriter(os.Stdout, 0, 0, 2, ' ', 0)
			for _, m := range entries {
				idStr := m.ShortID()
				if fullFlag {
					idStr = m.DisplayID()
				}

				tsStr := formatTS(parseTS(m.TS), now, dateMode)

				if previewChars > 0 {
					preview, err := store.Preview(m.ID, previewChars)
					if err != nil {
						preview = ""
					}
					fmt.Fprintf(w, "%s\t%s\t%s\t%s\n", idStr, store.HumanSize(m.Size), tsStr, preview)
				} else {
					fmt.Fprintf(w, "%s\t%s\t%s\n", idStr, store.HumanSize(m.Size), tsStr)
				}
			}
			return w.Flush()
		},
	}

	cmd.Flags().BoolVar(&fullFlag, "full", false, "Show full canonical ULIDs")
	cmd.Flags().Bool("short", false, "Force short-ID display (default)")
	cmd.Flags().IntVar(&previewChars, "preview-chars", 80, "Number of content characters to preview (0 to disable)")
	cmd.Flags().StringVar(&dateMode, "date", "relative", "Date format: relative or absolute")
	return cmd
}
