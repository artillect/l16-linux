// l16-render IN.lri OUT.jpg|OUT.dng [SIZE]: render an LRI with Light's own renderer (libcp,
// from the camera's stock system), the way the stock gallery's native code does
// (libnative-lib: CIAPI::Renderer::Create, input stream with the hot-pixel map, tuning,
// writeImage). An Android (bionic) program: built with the NDK, run through the stock
// linker64 and libraries copied to /var/lib/l16/android (light-lfc-android-libs), with the
// camera's hot-pixel map from there too. Run it through the l16-render script. SIZE: the long side in pixels
// (default: the renderer's full size).
#include <cstdio>
#include <cstring>
#include <algorithm>
#include <cstdlib>
#include <istream>
#include <ostream>
#include <streambuf>
#include <functional>
#include <memory>
#include <string>
#include <vector>

namespace CIAPI {
enum RendererProfile { Thumbnail, DeviceFL5, DeviceL16, Desktop };
enum ParamInt { ChannelOrder = 10, DisableNullUpdates = 11, ExportAtHighQuality = 12,
		ExportCompressionQualityJPEG = 16, ExportColorSpace = 19 };
enum ParamFloat { ViewDofFNumber = 0 };
enum ParamFloatArray { FocusMaskColor = 5 };
enum ExportImageFormat { JPEG, PPM, DNG, HDR, JPEG_GDEPTH };
enum TuningType { TuningDefault = 0, TuningGallery = 1 };
struct RectF { float x, y, w, h; };
template <class T> struct Point { T x, y; };
// opaque storage: the classes' own code does everything (sizes from the disassembly)
class Image {
	alignas(8) unsigned char d_[16];
public:
	Image(const Image &);
	~Image();
	int width() const;
	int height() const;
};
class ImagePyramid {
	alignas(8) unsigned char d_[16];
public:
	ImagePyramid(const ImagePyramid &);
	~ImagePyramid();
	Image &operator[](int);
	int levelCount() const;
};
class Transform {
	alignas(8) unsigned char d_[16];
public:
	RectF crop() const;
};
class RendererBase {
	alignas(8) unsigned char d_[16];
public:
	virtual ~RendererBase();
	void setProperty(ParamInt, int);
	void setProperty(ParamFloat, float);
	void setProperty(ParamFloatArray, const std::vector<float> &);
	void setInputDataStream(const std::shared_ptr<std::istream> &);
	bool isCompatible() const;
	Transform &transform();
};
class Renderer : public RendererBase {
public:
	static Renderer Create(RendererProfile);
	Renderer(const Renderer &);
	~Renderer();
	ImagePyramid outputBuffer() const;
	bool writeImage(const std::shared_ptr<std::ostream> &, const Point<int> &, ExportImageFormat,
			std::function<void(int)>);
	void cancelRenderRequests();
};
const char *GetVersion();
void ApplyTuning(TuningType, RendererBase &);
std::shared_ptr<std::streambuf> CreateMultiStream(const std::vector<std::shared_ptr<std::istream>> &);
}

using namespace CIAPI;

// Files as streams for libcp without std::fstream: the NDK's headers expect a newer C++
// runtime to provide the file streams, and libcp needs the stock one (theirs is inlined in
// the old library's users). A buffered FILE*, seekable (the LRI reader jumps about).
class FileBuf : public std::streambuf {
	FILE *f_;
	char buf_[1 << 16];
public:
	FileBuf(const char *path, const char *mode) : f_(std::fopen(path, mode)) {}
	~FileBuf() override { sync(); if (f_) std::fclose(f_); }
	bool ok() const { return f_ != nullptr; }
protected:
	int_type underflow() override {
		size_t n = std::fread(buf_, 1, sizeof(buf_), f_);
		if (n == 0)
			return traits_type::eof();
		setg(buf_, buf_, buf_ + n);
		return traits_type::to_int_type(buf_[0]);
	}
	pos_type seekoff(off_type off, std::ios_base::seekdir dir, std::ios_base::openmode) override {
		sync();
		if (dir == std::ios_base::cur)	// the position libcp sees is after what it has read
			off -= egptr() - gptr();
		setg(buf_, buf_, buf_);
		if (fseeko(f_, off, dir == std::ios_base::beg ? SEEK_SET : dir == std::ios_base::cur ? SEEK_CUR : SEEK_END))
			return pos_type(off_type(-1));
		return pos_type(off_type(ftello(f_)));
	}
	pos_type seekpos(pos_type pos, std::ios_base::openmode which) override {
		return seekoff(off_type(pos), std::ios_base::beg, which);
	}
	std::streamsize xsputn(const char *s, std::streamsize n) override {
		return std::fwrite(s, 1, n, f_);
	}
	int_type overflow(int_type c) override {
		if (c != traits_type::eof() && std::fputc(c, f_) == EOF)
			return traits_type::eof();
		return traits_type::not_eof(c);
	}
	int sync() override { return f_ && std::fflush(f_) == 0 ? 0 : -1; }
};

// an istream/ostream that owns its FileBuf
template <class S> struct FileStream : S {
	FileBuf fb;
	FileStream(const char *path, const char *mode) : S(nullptr), fb(path, mode) { this->rdbuf(&fb); }
};

int main(int argc, char **argv)
{
	if (argc < 3) {
		std::fprintf(stderr, "usage: l16-render IN.lri OUT.jpg|OUT.dng [LONG-SIDE]\n");
		return 64;
	}
	const std::string out = argv[2];
	const int want = argc > 3 ? std::atoi(argv[3]) : 0;
	std::fprintf(stderr, "libcp %s\n", GetVersion());

	Renderer r = Renderer::Create(DeviceL16);
	r.setProperty(FocusMaskColor, std::vector<float>{0.25f, 0.25f, 0.25f, 0.75f});

	// the LRI, and the camera's hot-pixel map after it (as the gallery does)
	std::vector<std::shared_ptr<std::istream>> parts;
	auto lri = std::make_shared<FileStream<std::istream>>(argv[1], "rb");
	if (!lri->fb.ok()) {
		std::fprintf(stderr, "can't open %s\n", argv[1]);
		return 1;
	}
	parts.push_back(lri);
	const char *hotpixel = std::getenv("L16_HOTPIXEL");
	auto hp = std::make_shared<FileStream<std::istream>>(
		hotpixel ? hotpixel : "/var/lib/l16/android/hotpixel.rec", "rb");
	if (hp->fb.ok())
		parts.push_back(hp);
	std::shared_ptr<std::streambuf> buf = CreateMultiStream(parts);
	auto in = std::make_shared<std::istream>(buf.get());
	r.setInputDataStream(in);
	ApplyTuning(TuningGallery, r);
	if (!r.isCompatible()) {
		std::fprintf(stderr, "libcp: not a compatible LRI\n");
		return 2;
	}
	r.setProperty(ExportCompressionQualityJPEG, 95);

	ImagePyramid pyr = r.outputBuffer();
	for (int l = 0; l < pyr.levelCount(); l++)
		std::fprintf(stderr, "level %d: %dx%d\n", l, pyr[l].width(), pyr[l].height());
	// the full-size level (the gallery scales by transform().crop(), which only differs
	// for another aspect ratio; its return convention isn't known yet)
	Point<int> size{ pyr[0].width(), pyr[0].height() };
	if (want > 0 && want < std::max(size.x, size.y)) {
		float k = float(want) / std::max(size.x, size.y);
		size = Point<int>{ int(size.x * k), int(size.y * k) };
	}
	std::fprintf(stderr, "writing %dx%d\n", size.x, size.y);

	bool dng = out.size() > 4 && out.compare(out.size() - 4, 4, ".dng") == 0;
	auto file = std::make_shared<FileStream<std::ostream>>(out.c_str(), "wb");
	if (!file->fb.ok()) {
		std::fprintf(stderr, "can't write %s\n", out.c_str());
		return 1;
	}
	std::shared_ptr<std::ostream> os = file;
	bool ok = r.writeImage(os, size, dng ? DNG : JPEG,
			       [](int p) { std::fprintf(stderr, "%d%%\n", p); });
	os->flush();
	r.cancelRenderRequests();
	r.setInputDataStream(std::shared_ptr<std::istream>());
	return ok ? 0 : 3;
}
