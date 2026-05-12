import { useCallback, useEffect, useMemo, type ChangeEvent } from "react";
import debounce from "lodash.debounce";

type FetchItems = (options?: { search?: string }) => Promise<void>;

interface UseSearchDebounceOptions {
  fetchItems: FetchItems;
  setSearchQuery: (query: string) => void;
}

export function useSearchDebounce({ fetchItems, setSearchQuery }: UseSearchDebounceOptions) {
  const debouncedSearch = useMemo(
    () => debounce(() => {
      fetchItems();
    }, 300),
    [fetchItems],
  );

  useEffect(() => {
    return () => {
      debouncedSearch.cancel();
    };
  }, [debouncedSearch]);

  const handleSearchChange = useCallback((e: ChangeEvent<HTMLInputElement>) => {
    setSearchQuery(e.target.value);
    debouncedSearch();
  }, [debouncedSearch, setSearchQuery]);

  const clearSearch = useCallback(() => {
    setSearchQuery("");
    fetchItems({ search: "" });
  }, [fetchItems, setSearchQuery]);

  return { handleSearchChange, clearSearch };
}