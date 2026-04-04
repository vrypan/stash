package cmd

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/spf13/cobra"
	"stash/store"
)

func newPushCmd() *cobra.Command {
	var attrFlags []string

	cmd := &cobra.Command{
		Use:           "push [file]",
		Short:         "Read stdin and create a new entry (default command)",
		SilenceUsage:  true,
		SilenceErrors: true,
		Args:          cobra.MaximumNArgs(1),
		RunE: func(c *cobra.Command, args []string) error {
			return runPushWithAttrs(c, args, attrFlags)
		},
	}

	cmd.Flags().StringArrayVarP(&attrFlags, "attr", "a", nil, "Attribute key=value (repeatable)")
	return cmd
}

func runPush(c *cobra.Command, args []string) error {
	return runPushWithAttrs(c, args, nil)
}

func parseAttrFlags(attrFlags []string) (map[string]string, error) {
	if len(attrFlags) == 0 {
		return nil, nil
	}
	attrs := make(map[string]string, len(attrFlags))
	for _, kv := range attrFlags {
		k, v, ok := strings.Cut(kv, "=")
		if !ok {
			return nil, fmt.Errorf("invalid --attr value %q: expected key=value", kv)
		}
		attrs[k] = v
	}
	return attrs, nil
}

func runPushWithAttrs(_ *cobra.Command, args []string, attrFlags []string) error {
	attrs, err := parseAttrFlags(attrFlags)
	if err != nil {
		return err
	}

	var (
		r *os.File
	)
	if len(args) == 1 {
		r, err = os.Open(args[0])
		if err != nil {
			return err
		}
		defer r.Close()
		if attrs == nil {
			attrs = make(map[string]string, 1)
		}
		if _, ok := attrs["filename"]; !ok {
			attrs["filename"] = filepath.Base(args[0])
		}
	} else {
		stat, err := os.Stdin.Stat()
		if err != nil {
			return err
		}
		if stat.Mode()&os.ModeCharDevice != 0 {
			return fmt.Errorf("no stdin provided")
		}
		r = os.Stdin
	}

	id, err := store.Push(r, attrs)
	if err != nil {
		return err
	}
	fmt.Println(strings.ToLower(id))
	return nil
}
