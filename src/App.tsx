// src/App.tsx
import { createSignal, Show, For, onMount, onCleanup } from 'solid-js';
import { invoke } from '@tauri-apps/api/core';

type SearchResult = {
  mft_entry: number;
  name: string;
};

function App() {
  const [query, setQuery] = createSignal('');
  const [results, setResults] = createSignal<SearchResult[]>([]);
  const [selectedIndex, setSelectedIndex] = createSignal(0);
  const [isLoading, setIsLoading] = createSignal(false);
  const [scanStatus, setScanStatus] = createSignal('');
  const [isScanning, setIsScanning] = createSignal(false);

  let inputRef: HTMLInputElement | undefined;

  onMount(() => {
    setTimeout(() => inputRef?.focus(), 50);
  });

  const handleFocus = () => {
    setTimeout(() => inputRef?.focus(), 50);
  };

  // --- Search ---
  const performSearch = async (q: string) => {
    if (q.trim().length < 2) {
      setResults([]);
      setSelectedIndex(0);
      return;
    }

    setIsLoading(true);
    try {
      const res = await invoke<SearchResult[]>('search_files', {
        query: q,
        limit: 50,
      });
      console.log('Search results:', res);
      setResults(res);
      setSelectedIndex(0);
    } catch (error) {
      console.error('Search failed:', error);
      setResults([]);
    } finally {
      setIsLoading(false);
    }
  };

  let timeoutId: ReturnType<typeof setTimeout> | undefined;
  const handleInput = (e: Event) => {
    const target = e.currentTarget as HTMLInputElement;
    const value = target.value;
    setQuery(value);

    clearTimeout(timeoutId);
    timeoutId = setTimeout(() => {
      performSearch(value);
    }, 200);
  };

  // --- Keyboard navigation ---
  const handleKeyDown = (e: KeyboardEvent) => {
    const resultsLen = results().length;

    switch (e.key) {
      case 'ArrowDown':
        e.preventDefault();
        if (resultsLen > 0) {
          setSelectedIndex((prev) => Math.min(prev + 1, resultsLen - 1));
        }
        break;
      case 'ArrowUp':
        e.preventDefault();
        if (resultsLen > 0) {
          setSelectedIndex((prev) => Math.max(prev - 1, 0));
        }
        break;
      case 'Enter':
        e.preventDefault();
        const selected = results()[selectedIndex()];
        if (selected) {
          invoke('open_file', { mftEntry: selected.mft_entry });
        }
        break;
      case 'Escape':
        window.dispatchEvent(new Event('blur'));
        break;
    }
  };

  // --- Scan ---
  const handleScan = async () => {
    setIsScanning(true);
    setScanStatus('Scanning MFT...');
    try {
      const result = await invoke<string>('rescan_index');
      setScanStatus(`✅ ${result}`);
      // Optionally clear search results
      setResults([]);
      setQuery('');
    } catch (error) {
      setScanStatus(`❌ Error: ${error}`);
    } finally {
      setIsScanning(false);
    }
  };

  onCleanup(() => {
    clearTimeout(timeoutId);
  });

  return (
    <div
      class="w-screen h-screen flex items-center justify-center bg-black/50 backdrop-blur-xl"
      onFocus={handleFocus}
    >
      <div class="w-[640px] max-w-[90vw] bg-[rgba(40,40,40,0.92)] backdrop-blur-2xl rounded-2xl shadow-2xl overflow-hidden border border-white/10">
        {/* Search Input */}
        <div class="flex items-center px-5 py-4 border-b border-white/10">
          <svg
            class="w-5 h-5 text-gray-400 flex-shrink-0 mr-3"
            fill="none"
            stroke="currentColor"
            viewBox="0 0 24 24"
          >
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
          </svg>
          <input
            ref={inputRef}
            type="text"
            placeholder="Search files..."
            value={query()}
            onInput={handleInput}
            onKeyDown={handleKeyDown}
            class="w-full bg-transparent text-white text-lg outline-none placeholder-gray-400 font-light"
            autofocus
          />
          <Show when={isLoading()}>
            <div class="w-4 h-4 border-2 border-white/20 border-t-white rounded-full animate-spin ml-2 flex-shrink-0" />
          </Show>
        </div>

        {/* Results List */}
        <Show when={results().length > 0}>
          <ul class="max-h-80 overflow-y-auto py-2">
            <For each={results()}>
              {(result, index) => (
                <li
                  class={`
                    px-5 py-2.5 flex items-center text-white cursor-pointer transition-colors
                    ${index() === selectedIndex() ? 'bg-blue-600/80' : 'hover:bg-white/10'}
                  `}
                  onMouseEnter={() => setSelectedIndex(index())}
                  onClick={() => invoke('open_file', { mftEntry: result.mft_entry })}
                >
                  <span class="text-sm truncate">
                    {result.name || `(Entry #${result.mft_entry})`}
                  </span>
                  <span class="ml-auto text-xs text-gray-400 opacity-60">
                    #{result.mft_entry}
                  </span>
                </li>
              )}
            </For>
          </ul>
        </Show>

        <Show when={query().trim().length > 0 && query().trim().length < 2}>
          <div class="px-5 py-4 text-gray-400 text-sm">Type at least 2 characters to search.</div>
        </Show>
        <Show when={query().trim().length >= 2 && !isLoading() && results().length === 0}>
          <div class="px-5 py-4 text-gray-400 text-sm">No results found.</div>
        </Show>

        {/* Footer */}
        <div class="px-5 py-2 border-t border-white/5 flex justify-between items-center text-xs text-gray-500">
          <span>↑↓ Navigate &nbsp;·&nbsp; ⏎ Open &nbsp;·&nbsp; ⎋ Hide</span>
          <button
            onClick={handleScan}
            disabled={isScanning()}
            class={`
              px-3 py-1 rounded-md text-xs transition-colors
              ${isScanning() 
                ? 'bg-gray-600/50 cursor-not-allowed' 
                : 'bg-blue-600 hover:bg-blue-700 text-white'}
            `}
          >
            {isScanning() ? 'Scanning...' : 'Scan'}
          </button>
        </div>

        <Show when={scanStatus()}>
          <div class="px-5 py-1 text-xs text-gray-400 border-t border-white/5 bg-white/5">
            {scanStatus()}
          </div>
        </Show>
      </div>
    </div>
  );
}

export default App;