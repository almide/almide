# http

HTTP client and server. import http, effect.

### `http.serve(port: Int, f: Fn[Unknown] -> Unknown) -> Unit`

Start an HTTP server on the given port with a request handler

```almd
http.serve(3000, (req) => http.response(200, "ok"))
```

### `http.response(status: Int, body: String) -> Response`

Create a plain text HTTP response with status code

```almd
http.response(200, "Hello!")
```

### `http.json(status: Int, body: String) -> Response`

Create a JSON HTTP response with status code

```almd
http.json(200, json.stringify(data))
```

### `http.with_headers(status: Int, body: String, headers: Map[String, String]) -> Response`

Create a response with custom headers

```almd
http.with_headers(200, body, {"Content-Type": "text/html"})
```

### `http.redirect(url: String) -> Response`

Create a 302 temporary redirect response

```almd
http.redirect("/new-path")
```

### `http.status(resp: Response, code: Int) -> Response`

Set the status code on a response

```almd
http.status(resp, 201)
```

### `http.body(resp: Response) -> String`

Get the body string from a response

```almd
let text = http.body(resp)
```

### `http.set_header(resp: Response, key: String, value: String) -> Response`

Set a header on a response

```almd
http.set_header(resp, "X-Custom", "value")
```

### `http.get_header(resp: Response, key: String) -> Option[String]`

Get a header value from a response

```almd
let ct = http.get_header(resp, "Content-Type")
```

### `http.req_method(req: Request) -> String`

Get the HTTP method of a request (GET, POST, etc.)

```almd
let method = http.req_method(req)
```

### `http.req_path(req: Request) -> String`

Get the URL path of a request

```almd
let path = http.req_path(req)
```

### `http.req_body(req: Request) -> String`

Get the body string of a request

```almd
let body = http.req_body(req)
```

### `http.req_header(req: Request, key: String) -> Option[String]`

Get a header value from a request

```almd
let auth = http.req_header(req, "Authorization")
```

### `http.query_params(req: Request) -> Map[String, String]`

Get all query parameters from a request as a map

```almd
let params = http.query_params(req) // {"page": "1", "q": "test"}
```

### `http.get(url: String) -> Result[String, String]`

Send an HTTP GET request and return the response body

```almd
let html = http.get("https://example.com")
```

### `http.post(url: String, body: String) -> Result[String, String]`

Send an HTTP POST request with a body string

```almd
let resp = http.post("https://api.example.com", body)
```

### `http.put(url: String, body: String) -> Result[String, String]`

Send an HTTP PUT request

```almd
let resp = http.put(url, body)
```

### `http.patch(url: String, body: String) -> Result[String, String]`

Send an HTTP PATCH request

```almd
let resp = http.patch(url, body)
```

### `http.delete(url: String) -> Result[String, String]`

Send an HTTP DELETE request

```almd
let resp = http.delete(url)
```

### `http.request(method: String, url: String, body: String, headers: Map[String, String]) -> Result[String, String]`

Send a custom HTTP request with method, URL, body, and headers

```almd
let resp = http.request("PUT", url, body, headers)
```
