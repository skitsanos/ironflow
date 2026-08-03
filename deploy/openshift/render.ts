const imageMarker = "invalid.invalid/ironflow:replace-with-immutable-digest";
const digestImagePattern =
  /^ghcr\.io\/skitsanos\/ironflow@sha256:[0-9a-f]{64}$/;

export function renderCanary(template: string, imageRef: string): string {
  if (!digestImagePattern.test(imageRef)) {
    throw new Error(
      "image must be ghcr.io/skitsanos/ironflow pinned by a sha256 digest",
    );
  }

  const markerCount = template.split(imageMarker).length - 1;
  if (markerCount !== 1) {
    throw new Error(
      `expected exactly one immutable-image marker, found ${markerCount}`,
    );
  }

  return template.replace(imageMarker, imageRef);
}

if (import.meta.main) {
  const imageRef = Bun.argv[2];
  if (!imageRef) {
    throw new Error(
      "usage: bun run deploy/openshift/render.ts ghcr.io/skitsanos/ironflow@sha256:<digest>",
    );
  }

  const template = await Bun.file(
    new URL("canary.yaml", import.meta.url),
  ).text();
  process.stdout.write(renderCanary(template, imageRef));
}
