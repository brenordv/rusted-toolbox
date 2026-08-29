# Basic
[x] Migrate cli to the new pattern.
[ ] How to consolidate the CLI arguments to allow for CommonToolArgs to be used here just like it is on all other (migrated) tools.
[ ] Add more test coverage to the tool.
[ ] Research improvements to the tool.
[ ] Include a plan for issue 41: inline variable feed
````
Add the ability to define request variables while importing it.
Example:
```
# @include my-get-request -> { queryStringType="question", correctAnswer=42, extra={{user}} }
```
In the example above, the request `my-get-request` would receive 3 variables:
1. `queryStringType` with value `question`.
2. `correctAnswer` with value `42`.
3. `extra` which uses another variable called `user`, requiring it to exist.

The arrow operator (`->`) could be named/referred to as variable feed.
````
[ ] Include a plan for issue 40: Enable stress/load test support
````
https://hurl.dev/docs/running-tests.html
````
[ ] Include a plan for issue 39: Environment override
````
New feature

When including a call inside another, add a syntax to allow for overriding environments: "For that specific request, for a specific include, environment local == dev".
````
[ ] Include a plan for issue 38: Elapsed time improvement
````
# New feature
Show the elapsed time each request took, and the total so far.
Pattern like: `[Elapsed: 200 ms | Total: 1.2 s]`

## Style points:
1. Keep track of the calls to the full url + env.
2. When making a new request, compare the current elapsed time with previous ones to see if it improved or not.
Example: `[Elapsed: 200 ms (+50 ms) | Total: 1.2 s]`
````
[ ] Include a plan for a new feat request: Add support for evaluating environment variables by executing a shell command, so we cna fetch api keys from something like Azure Key Vault.
[ ] Include a plan for a new feat request: A way to cache variables we fetch from other APIs. For instance: cache an auth token and re-use it for multiple requests during the same runtime instead of requesting a new token every time. We don't need to persist cross-runtime sessions.