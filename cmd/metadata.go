package cmd

import (
	"fmt"
	"sort"
	"strings"

	"stash/store"
)

func runMetadataSet(id string, args []string) error {
	if len(args) == 0 {
		return fmt.Errorf("metadata set requires at least one key=value pair")
	}
	attrs := make(map[string]string, len(args))
	for _, kv := range args {
		k, v, ok := strings.Cut(kv, "=")
		if !ok {
			return fmt.Errorf("invalid metadata value %q: expected key=value", kv)
		}
		attrs[k] = v
	}
	return store.WithLock(func() error {
		return store.SetAttrs(id, attrs)
	})
}

func runMetadataUnset(id string, args []string) error {
	if len(args) == 0 {
		return fmt.Errorf("metadata unset requires at least one key")
	}
	keys := append([]string(nil), args...)
	sort.Strings(keys)
	return store.WithLock(func() error {
		return store.UnsetAttrs(id, keys)
	})
}
