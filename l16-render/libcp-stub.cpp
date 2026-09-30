// Link-time stand-in for Light's libcp.so (the real one's symbol table trips the NDK's
// linker): the same exports, empty. At run time the stock libcp.so is loaded instead.
#include <istream>
#include <ostream>
#include <functional>
#include <memory>
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


namespace CIAPI {
Image::Image(const Image &) {}
Image::~Image() {}
int Image::width() const { return 0; }
int Image::height() const { return 0; }
ImagePyramid::ImagePyramid(const ImagePyramid &) {}
ImagePyramid::~ImagePyramid() {}
Image &ImagePyramid::operator[](int) { return *(Image *)nullptr; }
int ImagePyramid::levelCount() const { return 0; }
RectF Transform::crop() const { return {}; }
RendererBase::~RendererBase() {}
void RendererBase::setProperty(ParamInt, int) {}
void RendererBase::setProperty(ParamFloat, float) {}
void RendererBase::setProperty(ParamFloatArray, const std::vector<float> &) {}
void RendererBase::setInputDataStream(const std::shared_ptr<std::istream> &) {}
bool RendererBase::isCompatible() const { return false; }
Transform &RendererBase::transform() { return *(Transform *)nullptr; }
Renderer Renderer::Create(RendererProfile) { return *(Renderer *)nullptr; }
Renderer::Renderer(const Renderer &o) : RendererBase(o) {}
Renderer::~Renderer() {}
ImagePyramid Renderer::outputBuffer() const { return *(ImagePyramid *)nullptr; }
bool Renderer::writeImage(const std::shared_ptr<std::ostream> &, const Point<int> &, ExportImageFormat, std::function<void(int)>) { return false; }
void Renderer::cancelRenderRequests() {}
const char *GetVersion() { return ""; }
void ApplyTuning(TuningType, RendererBase &) {}
std::shared_ptr<std::streambuf> CreateMultiStream(const std::vector<std::shared_ptr<std::istream>> &) { return {}; }
}
