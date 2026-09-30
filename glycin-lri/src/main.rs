// glycin-lri: LRI files (Light L16 captures) in any glycin image app (Loupe, file manager
// thumbnails): the reference module's frame, debayered at half resolution with the file's
// own white balance. The full picture is the gallery's job (all modules fused).

mod lri;

use std::io::BufReader;

use glycin_utils::*;

init_main_loader!(LriLoader);

pub struct LriLoader {
    picture: lri::Picture,
}

impl LoaderImplementation for LriLoader {
    fn init(
        stream: UnixStream,
        _mime_type: String,
        _details: InitializationDetails,
    ) -> Result<(LriLoader, ImageDetails), ProcessError> {
        let picture = lri::quick(&mut BufReader::with_capacity(1 << 20, stream)).expected_error()?;
        let mut details = ImageDetails::new(picture.width, picture.height);
        details.info_format_name = Some(String::from("Light LRI"));
        Ok((LriLoader { picture }, details))
    }

    fn frame(&mut self, _frame_request: FrameRequest) -> Result<Frame, ProcessError> {
        let mut memory = SharedMemory::new(self.picture.rgb.len() as u64).expected_error()?;
        memory.copy_from_slice(&self.picture.rgb);
        Frame::new(
            self.picture.width,
            self.picture.height,
            MemoryFormat::R8g8b8,
            memory.into_binary_data(),
        )
        .internal_error()
    }
}
