package cmd

import (
	"os"
	"strconv"
	"strings"

	"github.com/spf13/cobra"
	"stash/store"
)

func resolveEntryRef(args []string) (string, error) {
	if len(args) == 0 {
		m, err := store.NthNewest(1)
		if err != nil {
			return "", err
		}
		return m.ID, nil
	}
	if strings.HasPrefix(args[0], "@") {
		return store.Resolve(args[0])
	}
	if n, err := strconv.Atoi(args[0]); err == nil {
		m, err := store.NthNewest(n)
		if err != nil {
			return "", err
		}
		return m.ID, nil
	}
	return store.Resolve(args[0])
}

func newCatCmd() *cobra.Command {
	return &cobra.Command{
		Use:           "cat [id|n|@n]",
		Aliases:       []string{"peek"},
		Short:         "Write the referenced entry's raw bytes to stdout",
		Args:          cobra.MaximumNArgs(1),
		SilenceUsage:  true,
		SilenceErrors: true,
		RunE: func(_ *cobra.Command, args []string) error {
			id, err := resolveEntryRef(args)
			if err != nil {
				return err
			}
			return store.Cat(id, os.Stdout)
		},
	}
}
