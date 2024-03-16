# bevy_defer_http

Http utilities for the `bevy_defer` crate, based on the `hyper` crate.

## Runtime

* The executor is the `futures` single threaded `LocalExecutor`.
* `async_io` is used as the reactor.

## Features

* [x] Http client.
* [ ] Https client.
* [ ] Server.
* [ ] WASM support.
