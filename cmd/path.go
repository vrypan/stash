package cmd

import (
	"bufio"
	"fmt"
	"os"
	"strings"

	"github.com/mattn/go-isatty"
	"github.com/spf13/cobra"
	"stash/store"
)

func newPathCmd() *cobra.Command {
	var dirMode bool

	cmd := &cobra.Command{
		Use:           "path [ref ...]",
		Short:         "Print absolute entry paths",
		Args:          cobra.ArbitraryArgs,
		SilenceUsage:  true,
		SilenceErrors: true,
		RunE: func(_ *cobra.Command, args []string) error {
			if len(args) > 0 {
				for _, ref := range args {
					if err := writeResolvedPath(ref, dirMode); err != nil {
						return err
					}
				}
				return nil
			}

			if isatty.IsTerminal(os.Stdin.Fd()) {
				path, err := pathFallback(dirMode)
				if err != nil {
					return err
				}
				fmt.Fprintln(os.Stdout, path)
				return nil
			}

			scanner := bufio.NewScanner(os.Stdin)
			seen := false
			for scanner.Scan() {
				ref := strings.TrimSpace(scanner.Text())
				if ref == "" {
					continue
				}
				seen = true
				if err := writeResolvedPath(ref, dirMode); err != nil {
					return err
				}
			}
			if err := scanner.Err(); err != nil {
				return err
			}
			if !seen {
				path, err := pathFallback(dirMode)
				if err != nil {
					return err
				}
				fmt.Fprintln(os.Stdout, path)
			}
			return nil
		},
	}

	cmd.Flags().BoolVarP(&dirMode, "dir", "d", false, "Print entry directories instead of data file paths")
	return cmd
}

func pathFallback(dirMode bool) (string, error) {
	if dirMode {
		return store.BaseDirPath()
	}
	return store.EntriesDirPath()
}

func writeResolvedPath(ref string, dirMode bool) error {
	id, err := resolveEntryRef([]string{ref})
	if err != nil {
		return err
	}
	var path string
	if dirMode {
		path, err = store.EntryDirPath(id)
	} else {
		path, err = store.EntryDataPath(id)
	}
	if err != nil {
		return err
	}
	fmt.Fprintln(os.Stdout, path)
	return nil
}
