<required_reading>
**Read these reference files NOW:**
1. references/command-reference.md (Inspection, Data Extraction sections)
2. references/snapshot-and-refs.md
</required_reading>

<process>

**Step 1: Navigate to the target page**

```bash
gsd-browser navigate <url>
gsd-browser wait-for --condition network_idle
```

**Step 2: Choose extraction method**

| Method | Best for |
|--------|----------|
| `extract --schema` | Structured data with known layout (product cards, listings) |
| `snapshot` | Quick overview of interactive elements |
| `accessibility-tree` | Full semantic structure (roles, names, states) |
| `find` | Locating specific elements by text, role, or selector |
| `page-source` | Raw HTML when other methods don't suffice |
| `eval` | Custom JavaScript extraction logic |

**Step 3a: Structured data extraction (preferred)**

Define a schema mapping field names to CSS selectors:

```bash
# Single item
gsd-browser extract --schema '{
  "type": "object",
  "properties": {
    "title": {"_selector": "h1", "_attribute": "textContent"},
    "price": {"_selector": ".price", "_attribute": "textContent"},
    "image": {"_selector": "img.product", "_attribute": "src"}
  }
}'
```

For lists of items, use `--selector` to target the repeating container and `--multiple`:

```bash
gsd-browser extract --selector ".product-card" --multiple --schema '{
  "type": "object",
  "properties": {
    "name": {"_selector": "h3", "_attribute": "textContent"},
    "price": {"_selector": ".price", "_attribute": "textContent"},
    "link": {"_selector": "a", "_attribute": "href"}
  }
}'
```

**Step 3b: Accessibility tree**

For understanding page structure without knowing selectors:

```bash
gsd-browser accessibility-tree
```

Returns roles, names, states, and hierarchy. Useful for discovering what's on the page before building a schema.

**Step 3c: Find elements**

Search by text, role, or selector:

```bash
gsd-browser find --text "Sign In"
gsd-browser find --role button
gsd-browser find --selector ".product-card" --limit 50
```

**Step 3d: Page source**

Raw HTML for complex extraction:

```bash
gsd-browser page-source
gsd-browser page-source --selector "main"    # Scoped to a section
```

**Step 3e: JavaScript evaluation**

For custom logic that other methods can't handle:

```bash
gsd-browser eval 'document.querySelectorAll(".item").length'
gsd-browser eval 'JSON.stringify([...document.querySelectorAll("a")].map(a => ({text: a.textContent, href: a.href})))'
```

**Step 4: Handle pagination**

For multi-page scraping:

```bash
# Extract page 1
gsd-browser extract --selector ".item" --multiple --schema '...'

# Navigate to next page
gsd-browser act --intent pagination_next
gsd-browser wait-for --condition network_idle

# Extract page 2
gsd-browser extract --selector ".item" --multiple --schema '...'
```

**Step 5: Handle frames/iframes**

If content is inside an iframe:

```bash
gsd-browser list-frames
gsd-browser select-frame --name "content-frame"
# Now extract from within the frame
gsd-browser extract --schema '...'
gsd-browser select-frame --name main              # Return to main frame
```

</process>

<common_patterns>

<pattern name="scrape_product_listing">
```bash
gsd-browser navigate https://store.example.com/products
gsd-browser wait-for --condition network_idle
gsd-browser extract --selector ".product" --multiple --schema '{
  "type": "object",
  "properties": {
    "name": {"_selector": ".title", "_attribute": "textContent"},
    "price": {"_selector": ".price", "_attribute": "textContent"},
    "url": {"_selector": "a", "_attribute": "href"},
    "image": {"_selector": "img", "_attribute": "src"}
  }
}' --json
```
</pattern>

<pattern name="extract_table_data">
```bash
gsd-browser navigate https://example.com/data
gsd-browser extract --selector "table tbody tr" --multiple --schema '{
  "type": "object",
  "properties": {
    "col1": {"_selector": "td:nth-child(1)", "_attribute": "textContent"},
    "col2": {"_selector": "td:nth-child(2)", "_attribute": "textContent"},
    "col3": {"_selector": "td:nth-child(3)", "_attribute": "textContent"}
  }
}' --json
```
</pattern>

<pattern name="discover_then_extract">
```bash
gsd-browser navigate https://example.com
gsd-browser accessibility-tree           # Understand structure
gsd-browser find --role heading          # Find headings
gsd-browser find --role link --limit 50  # Find all links
# Then build schema based on what you found
```
</pattern>

</common_patterns>

<success_criteria>
Data extraction workflow is complete when:
- Target data is extracted in the expected format
- All pages/items are captured (pagination handled if needed)
- JSON output is valid and contains expected fields
- Frames are handled if content is in iframes
</success_criteria>
