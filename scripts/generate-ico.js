const fs = require('fs');
const path = require('path');

const iconPath = path.join(__dirname, '../apps/desktop/src-tauri/icons/icon.png');
const icoPath = path.join(__dirname, '../apps/desktop/src-tauri/icons/icon.ico');

// Check if source png exists
if (!fs.existsSync(iconPath)) {
    console.error('Source icon.png not found');
    process.exit(1);
}

const pngData = fs.readFileSync(iconPath);
const size = pngData.length;

// Create ICO header
// Header: 6 bytes
// 0-1: Reserved (0)
// 2-3: Type (1 = ICO)
// 4-5: Count (1 image)
const header = Buffer.alloc(6);
header.writeUInt16LE(0, 0);
header.writeUInt16LE(1, 2);
header.writeUInt16LE(1, 4);

// Create Directory Entry
// Entry: 16 bytes
// 0: Width (0 for 256)
// 1: Height (0 for 256)
// 2: Color Count (0)
// 3: Reserved (0)
// 4-5: Planes (1)
// 6-7: BitCount (32)
// 8-11: Size (PNG size)
// 12-15: Offset (Header 6 + Directory 16 = 22)
const entry = Buffer.alloc(16);
entry.writeUInt8(0, 0); // Width 256 -> 0
entry.writeUInt8(0, 1); // Height 256 -> 0
entry.writeUInt8(0, 2); // Colors
entry.writeUInt8(0, 3); // Reserved
entry.writeUInt16LE(1, 4); // Planes
entry.writeUInt16LE(32, 6); // BitCount
entry.writeUInt32LE(size, 8); // Size
entry.writeUInt32LE(22, 12); // Offset

const icoData = Buffer.concat([header, entry, pngData]);

fs.writeFileSync(icoPath, icoData);
console.log(`Generated ${icoPath} (${icoData.length} bytes)`);
