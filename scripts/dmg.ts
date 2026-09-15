#!/usr/bin/env bun

// Styled DMG creation built directly on hdiutil.
//
// Replaces create-dmg, whose interstitial mount discovery
// (`hdiutil attach -mountrandom` + parsing `hdiutil info`) fails
// deterministically on some CI runners with:
//   "unable to proceed with final disk image creation because the
//    interstitial disk image was not found."
// Mounting at an explicit mountpoint removes that failure mode entirely.

import { $ } from "bun";
import { mkdir, mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { basename, dirname, join } from "node:path";

export type StyledDmgOptions = {
  /** Directory whose contents become the DMG root (holds `<app>.app`). */
  stagingDirectory: string;
  /** Final `.dmg` path. Parent directories are created. */
  outputPath: string;
  /** Mounted volume name shown in Finder. */
  volumeName: string;
  /** e.g. `Insulator.app`. */
  appFileName: string;
  /** Finder window geometry. */
  windowBounds?: { x: number; y: number; width: number; height: number };
  iconSize?: number;
  textSize?: number;
  appPosition?: { x: number; y: number };
  applicationsPosition?: { x: number; y: number };
};

async function detachDisk(device: string): Promise<void> {
  let lastDetail = "";
  for (let attempt = 1; attempt <= 5; attempt++) {
    const result = await $`hdiutil detach ${device}`.quiet().nothrow();
    if (result.exitCode === 0) {
      return;
    }
    lastDetail = (
      result.stderr.toString() || result.stdout.toString()
    ).trim();
    if (attempt < 5) {
      await Bun.sleep(1000 * 2 ** attempt);
    }
  }
  const forced = await $`hdiutil detach -force ${device}`
    .quiet()
    .nothrow();
  if (forced.exitCode === 0) {
    console.warn(`hdiutil detach ${device} required -force.`);
    return;
  }
  throw new Error(`hdiutil detach ${device} failed: ${lastDetail}`);
}

function finderScript(
  options: Required<StyledDmgOptions>,
  finderDiskName: string,
): string {
  const left = options.windowBounds.x;
  const top = options.windowBounds.y;
  const right = left + options.windowBounds.width;
  const bottom = top + options.windowBounds.height;
  return `tell application "Finder"
	tell disk "${finderDiskName}"
		open
		set current view of container window to icon view
		set toolbar visible of container window to false
		set statusbar visible of container window to false
		set the bounds of container window to {${left}, ${top}, ${right}, ${bottom}}
		set theViewOptions to the icon view options of container window
		set arrangement of theViewOptions to not arranged
		set icon size of theViewOptions to ${options.iconSize}
		set text size of theViewOptions to ${options.textSize}
		set position of item "${options.appFileName}" of container window to {${options.appPosition.x}, ${options.appPosition.y}}
		set position of item "Applications" of container window to {${options.applicationsPosition.x}, ${options.applicationsPosition.y}}
		set extension hidden of item "${options.appFileName}" of container window to true
		close
		open
		update without registering applications
		delay 2
	end tell
	delay 1
end tell
`;
}

export async function createStyledDmg(
  options: StyledDmgOptions,
): Promise<void> {
  const full: Required<StyledDmgOptions> = {
    windowBounds: { x: 200, y: 120, width: 660, height: 400 },
    iconSize: 128,
    textSize: 13,
    appPosition: { x: 180, y: 178 },
    applicationsPosition: { x: 480, y: 178 },
    ...options,
  };

  if (
    !full.volumeName.trim() ||
    full.volumeName.includes("/") ||
    full.volumeName.length > 27
  ) {
    throw new Error(
      "volumeName must be non-empty, at most 27 characters, and cannot contain '/'.",
    );
  }

  await mkdir(dirname(full.outputPath), { recursive: true });
  await rm(full.outputPath, { force: true });

  // Staged bytes plus slack for the Applications symlink, .DS_Store,
  // and APFS overhead.
  const duOutput = await $`du -sm ${full.stagingDirectory}`.quiet().text();
  const stagedMb = Number.parseInt(duOutput.split(/\s+/)[0] ?? "", 10);
  if (!Number.isSafeInteger(stagedMb) || stagedMb < 0) {
    throw new Error(`Cannot parse du output: ${JSON.stringify(duOutput)}`);
  }
  const imageMb = stagedMb + 48;

  const rwPath = join(
    dirname(full.outputPath),
    `rw.${process.pid}.${basename(full.outputPath)}`,
  );
  await rm(rwPath, { force: true });

  console.log(`Creating disk image (${imageMb} MB)...`);
  await $`hdiutil create -srcfolder ${full.stagingDirectory} -volname ${full.volumeName} -fs APFS -format UDRW -size ${`${imageMb}m`} ${rwPath}`;

  const mountPoint = join(
    "/Volumes",
    `${full.volumeName}-dmg-${process.pid}-${Math.floor(Math.random() * 1_000_000)}`,
  );
  if (!mountPoint.startsWith("/Volumes/")) {
    throw new Error(`Refusing to use unexpected mountpoint: ${mountPoint}`);
  }
  // The mountpoint directory is created by hdiutil/DiskArbitration on attach
  // (/Volumes is not user-writable for mkdir). Detach first in case a stale
  // mount lingers at this path from a previous crashed run.
  await $`hdiutil detach ${mountPoint}`.quiet().nothrow();
  let device: string | undefined;
  try {
    console.log("Mounting disk image...");
    const attachOutput = await $`hdiutil attach -readwrite -noverify -noautoopen -mountpoint ${mountPoint} ${rwPath}`.text();
    // APFS attaches can print several /dev lines; the volume we mounted is
    // the one whose line carries our mountpoint.
    const attachedLine =
      attachOutput.split("\n").find((line) => line.includes(mountPoint)) ??
      attachOutput.split("\n").filter((line) => line.startsWith("/dev/")).at(-1);
    const deviceMatch = attachedLine?.match(/^\/dev\/\S+/);
    if (!deviceMatch) {
      throw new Error(
        `Cannot parse hdiutil attach output: ${JSON.stringify(attachOutput)}`,
      );
    }
    device = deviceMatch[0];
    console.log(`Device name:     ${device}`);
    console.log(`Mount dir:       ${mountPoint}`);

    console.log("Making link to Applications dir...");
    await $`ln -s /Applications ${join(mountPoint, "Applications")}`;

    // Pause before AppleScript to work around occasional
    // "Can't get disk (-1728)" failures.
    await Bun.sleep(5000);
    const scriptDirectory = await mkdtemp(join(tmpdir(), "insulator-dmg-osa-"));
    try {
      const scriptPath = join(scriptDirectory, "style.scpt");
      await Bun.write(scriptPath, finderScript(full, basename(mountPoint)));
      console.log("Running AppleScript for Finder styling...");
      await $`osascript ${scriptPath}`;
    } finally {
      await rm(scriptDirectory, { force: true, recursive: true });
    }

    console.log("Fixing permissions...");
    await $`chmod -Rf go-w ${mountPoint}`.quiet();
    await $`rm -rf ${join(mountPoint, ".fseventsd")}`.quiet();
    // Let Finder flush .DS_Store before detach.
    await Bun.sleep(4000);
  } finally {
    if (device !== undefined) {
      console.log("Unmounting disk image...");
      await detachDisk(device);
    }
    await rm(mountPoint, { force: true, recursive: true });
  }

  console.log("Compressing disk image...");
  await $`hdiutil convert ${rwPath} -format ULFO -o ${full.outputPath}`;
  await rm(rwPath, { force: true });
  console.log("Disk image done");
}
