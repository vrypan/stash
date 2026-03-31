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

	cmd := &cobra.Command{
		Use:           "list",
		Short:         "List entries, newest first",
		SilenceUsage:  true,
		SilenceErrors: true,
		RunE: func(_ *cobra.Command, _ []string) error {
			entries, err := store.List()
			if err != nil {
				return err
			}

			w := tabwriter.NewWriter(os.Stdout, 0, 0, 2, ' ', 0)
			for _, m := range entries {
				idStr := m.ShortID()
				if fullFlag {
					idStr = m.ID
				}
				ts, err := time.Parse(time.RFC3339Nano, m.TS)
				if err != nil {
					ts, _ = time.Parse(time.RFC3339, m.TS)
				}
				fmt.Fprintf(w, "%s\t%s\t%s\n",
					idStr,
					store.HumanSize(m.Size),
					ts.UTC().Format(time.RFC3339),
				)
			}
			return w.Flush()
		},
	}

	cmd.Flags().BoolVar(&fullFlag, "full", false, "Show full canonical ULIDs")
	cmd.Flags().Bool("short", false, "Force short-ID display (default)")
	return cmd
}
