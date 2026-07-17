use embedded_storage::nor_flash::NorFlash;

pub struct Nvs<T: NorFlash>
{
    partition: T
}