# Image Processing Tool (imgx)
## What It Does
The Image Processing Tool (`imgx`) is a multithreaded command-line utility for batch image editing operations. 
The tool supports common image operations like resizing, grayscale conversion, and format conversion while maintaining
quality through advanced encoding algorithms.

**Key Features:**
- **Parallel Processing**: Multi-threaded image processing using all available CPU cores
- **Batch Operations**: Process multiple files or entire directories recursively  
- **Format Support**: Handles PNG, JPEG, GIF, WebP, AVIF, TIFF, BMP and more
- **Metadata Preservation**: Maintains ICC color profiles and EXIF orientation data
- **Progress Tracking**: Real-time progress bars for each file being processed
- **Quality Optimization**: Uses high-quality encoding algorithms (Lanczos3 for resizing, lossless WebP, etc.)
- **Smart Output Naming**: Automatically generates descriptive filenames based on operations performed

## Command-Line Options
- **Input Files**: Specify files or directories to process (supports recursive directory scanning)
- `-r, --resize <RESIZE>`: Resize by percentage or exact size.
  - Percent: `50`, `12.5`, `12.5%`
  - Exact size: `640,480`, `640.5,480.25`
  - Note: when using exact size, the tool warns if width/height ratios differ from the original image
- `-g, --grayscale`: Convert images to grayscale
- `-c, --convert <FORMAT>`: Convert images to specified format (png, jpg, webp, avif, gif, bmp, tiff, etc.)

## Examples
### Basic Image Resizing
**Command:**
```bash
imgx image1.jpg image2.png --resize 50
```
**Input**: `image1.jpg` (1920x1080), `image2.png` (1024x768)  
**Output**: 
- `image1-resized50.jpg` (960x540)
- `image2-resized50.png` (512x384)

### Resize With Decimals
**Command:**
```bash
imgx image1.jpg --resize 15.42%
```
**Input**: `image1.jpg` (1920x1080)  
**Output**: `image1-resized15.42pct.jpg` (~296x166)

### Resize to Exact Dimensions
**Command:**
```bash
imgx image1.jpg --resize 640,480
```
**Input**: `image1.jpg` (1920x1080)  
**Output**: `image1-resized640x480.jpg` (640x480)  
**Note**: A warning is logged if the resize ratios differ from the original

### Convert Images to Grayscale
**Command:**
```bash
imgx photos/*.jpg --grayscale
```
**Input**: Multiple JPEG files in the photos directory  
**Output**: Same files with `-grayscale` suffix in their names

### Batch Format Conversion
**Command:**
```bash
imgx *.png --convert webp
```
**Input**: All PNG files in current directory  
**Output**: WebP files with `-convertWebP` suffix, preserving original quality

### Complex Operation - Resize, Grayscale, and Convert
**Command:**
```bash
imgx vacation_photos/ --resize 75 --grayscale --convert jpg
```
**Input**: All supported images in `vacation_photos/` directory (recursive)  
**Output**: JPEG files at 75% size in grayscale with descriptive filenames like `photo001-resized75-grayscale-convertJpeg.jpg`

### Process Entire Directory Structure
**Command:**
```bash
imgx /path/to/image/collection --resize 80
```
**Input**: Recursively finds all supported image files in the directory tree  
**Output**: Resized versions of all images with preserved directory structure

## Known Issues
1. **Memory Usage**: Large images or batch operations may consume significant memory, especially with many parallel threads
2. **JPEG Transparency**: Transparent images are converted to RGB when saving as JPEG (format limitation)
3. **Format Support**: Some exotic image formats may fall back to basic encoding without advanced optimizations
