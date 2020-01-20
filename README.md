# Thea

Thea is a from-memory website generator and server designed to get the requested resource from server to client as quickly as possible, with as little setup as possible. Much like a static site generator, Thea takes HTML / Markdown / etc files and renders pages from them via liquid-based templates. Where a static site generator would write those pages to HTML files, Thea keeps them in, and serves them from, memory.

Design goals (in order):

1. Outstanding server to client performance.
2. Frustration-free setup and configuration.
3. Powerful, versatile templating [1].

## Performance Details

As stated, optimum server to client performance is Thea's goal and it employs a number of strategies to achieve that goal.

* Pages are kept in memory in a static HashMap.
* Requests are handled by a highly performant [Actix web server](https://github.com/actix/actix-web).
* During the initial page render:
    * HTML and XML pages are minified.
    * All pages are assigned a unique ETag.
* During a request:
    * ETags are strongly validated and Thea will return early with [304 Not Modified](https://developer.mozilla.org/en-US/docs/Web/HTTP/Status/304) if possible.
* During a response:
    * Thea retrieves the page's content from the static HashMap.
    * The ETag header is set.
    * A 15 minute CacheControl header is set.
    * The response is compressed with brotli.
* Extras:
    * The contents of the HashMap can optionally be written to disk, allowing utilities like purgecss to be used.

---

[1]: Template rendering only happens once during server startup. As such, Thea places a much higher importance on powerful templating features than it does on template rendering performance. That being said, template rendering is far from slow.
