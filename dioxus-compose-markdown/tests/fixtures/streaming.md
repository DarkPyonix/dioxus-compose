## A reply the way a model writes one

Sure. Here is the change, with **the important part in bold** and a
paragraph that wraps across two lines with *emphasis that
crosses the line break*.

[docs]: https://example.com/reference "Reference"

See the [reference][docs], the [inline link](https://example.com/a?b=c), an
autolink <https://example.com/auto>, and a bare one at https://example.com/bare.

1. First, edit `Cargo.toml`:

   ```toml
   [dependencies]
   dioxus-compose-markdown = "0.0.0"
   ```

2. Then run the tests.
   - nested **bullet**
   - another, with a hard break  
     on the next line

> Quoted text,
> continued lazily
over two lines.
>
> > And a quote inside a quote.

| Name | Value |
|------|------:|
| `a`  | 1     |
| b    | **2** |

```diff
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,3 +1,3 @@
 fn main() {
-    old();
+    new();
 }
```

<div class="raw">this is <b>raw</b> html</div>

An image: ![a cat](https://example.com/cat.png) and inline <span>html</span>.

Setext heading
--------------

***

Done &amp; dusted.
