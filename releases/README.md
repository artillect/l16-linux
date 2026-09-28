Release notes, one file per release: `<tag>.md` is the body of GitHub release `<tag>`.

To make a release: write the notes, commit them, then run the **Build** workflow by hand
(Actions → Build → Run workflow) with the tag, or:

```sh
gh workflow run build.yml -f release=<tag>
```

It builds the packages and the image, and publishes both.
