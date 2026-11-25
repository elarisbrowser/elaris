// SPDX-License-Identifier: GPL-3.0-only

#include "presentation_gpu_linux.h"

#include <AK/Vector.h>

#include <LibGfx/VulkanContext.h>

#include <string.h>
#include <sys/sysmacros.h>
#include <vulkan/vulkan.h>

namespace Elaris {

static bool device_has_extension(VkPhysicalDevice physical_device, char const* name) {
    uint32_t count = 0;
    vkEnumerateDeviceExtensionProperties(physical_device, nullptr, &count, nullptr);
    Vector<VkExtensionProperties> extensions;
    extensions.resize(count);
    vkEnumerateDeviceExtensionProperties(physical_device, nullptr, &count, extensions.data());
    for (auto const& extension : extensions) {
        if (strcmp(extension.extensionName, name) == 0)
            return true;
    }
    return false;
}

Optional<Gfx::GpuDeviceIdentity> presentation_gpu_identity(dev_t device) {
    VkApplicationInfo application{};
    application.sType = VK_STRUCTURE_TYPE_APPLICATION_INFO;
    application.apiVersion = VK_API_VERSION_1_1;
    VkInstanceCreateInfo create_info{};
    create_info.sType = VK_STRUCTURE_TYPE_INSTANCE_CREATE_INFO;
    create_info.pApplicationInfo = &application;
    VkInstance instance = VK_NULL_HANDLE;
    if (vkCreateInstance(&create_info, nullptr, &instance) != VK_SUCCESS)
        return {};

    uint32_t count = 0;
    vkEnumeratePhysicalDevices(instance, &count, nullptr);
    Vector<VkPhysicalDevice> physical_devices;
    physical_devices.resize(count);
    vkEnumeratePhysicalDevices(instance, &count, physical_devices.data());

    Optional<Gfx::GpuDeviceIdentity> identity;
    for (auto* physical_device : physical_devices) {
        if (!device_has_extension(physical_device, VK_EXT_PHYSICAL_DEVICE_DRM_EXTENSION_NAME))
            continue;
        VkPhysicalDeviceDrmPropertiesEXT drm{};
        drm.sType = VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_DRM_PROPERTIES_EXT;
        VkPhysicalDeviceProperties2 properties{};
        properties.sType = VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_PROPERTIES_2;
        properties.pNext = &drm;
        vkGetPhysicalDeviceProperties2(physical_device, &properties);
        // The window system may name the device by either of its nodes.
        auto is_primary = drm.hasPrimary && makedev(drm.primaryMajor, drm.primaryMinor) == device;
        auto is_render = drm.hasRender && makedev(drm.renderMajor, drm.renderMinor) == device;
        if (is_primary || is_render) {
            identity = Gfx::gpu_device_identity_of(physical_device);
            break;
        }
    }
    vkDestroyInstance(instance, nullptr);
    return identity;
}

} // namespace Elaris
