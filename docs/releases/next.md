# Next IronFlow Release - Draft

The approved 1.23.0 changes are recorded in [1.23.0.md](1.23.0.md).
Development versions remain prereleases until release-branch finalization;
publication is verified separately.

## Upgrade Notes

- **UTF-8 response headers are strings** (IF-264). In `{output_key}_headers`
  of `http_request` and its wrappers, a value whose bytes are valid UTF-8,
  such as `filename="résumé.pdf"` in `Content-Disposition`, is now a string.
  Since 1.23.0 it was `{"base64": "..."}` because it was not plain ASCII. A
  flow that decoded that object for such a header must read the string.
  Values that are not valid UTF-8, or that hold a C1 control character,
  keep the `{"base64": "..."}` form.

Earlier changes remain in [the 1.22.0 notes](1.22.0.md).
