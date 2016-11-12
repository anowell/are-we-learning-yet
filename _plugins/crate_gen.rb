require 'fileutils'
require 'json'
require 'yaml'
require 'net/http'

module Jekyll
  class CrateGenerator < Generator
    safe true
    priority :highest

    GH_OAUTH_TOKEN = ENV['GITHUB_OAUTH_TOKEN']

    def use_crate_cache?
      src_file = File.join(__dir__, "../_data/crates.yaml")
      gen_file = File.join(__dir__, "../_tmp/crates_generated.yaml")

      File.exists?(gen_file) && File.mtime(gen_file) >= File.mtime(src_file)
    end

    def generate(site)
      crates = use_crate_cache? ? read_crate_cache : generate_crate_data(site.data['crates'])
      site.data['crates_generated'] = crates
      save_crate_cache(crates) unless use_crate_cache?
    end

    def generate_crate_data(crates)
      puts "WARNING: GITHUB_OAUTH_TOKEN not set - you may get rate-limited by GitHub" unless GH_OAUTH_TOKEN
      crates.map do |crate|
        unless crate['name'] || crate['repository']
          puts "ERROR: crate entry is invalid: #{crate}"
          exit 1
        end

        puts "Processing #{crate['name'] || crate['repository']}"

        # Get data from the Crates.io API
        if crate['name']
          crate_data = get_crate_data(crate['name'])
          crate = crate_data.merge(crate)

          crate['documentation'] = "https://docs.rs/crate/#{crate['name']}" unless crate['documentation']
        end


        # Get data from the GitHub API
        matches = crate['repository']&.match(/github.com\/([^\.\/]+\/[^\.\/]+)/)
        repo = matches[1] if matches
        if repo
          repo_data = get_repo_data(repo)
          crate = repo_data.merge(crate)
          crate['github'] = repo
        else
          puts "WARNING: No GitHub repository specified for crate #{crate['name']}"
        end

        crate
      end
    end

    # Fetches a URL and caches the result as a tmp file for future calls
    # This allows rerunning this script without hammerring APIs
    def cached_request(url, headers={}, limit=3)
      if limit == 0
        puts "Error: reached retry limit"
        return {}
      end

      dir = url.sub(/^https?:\/\//, File.join(__dir__, "../_tmp/"))
      path = File.join(dir, "index.json")
      FileUtils.mkdir_p(dir)
      unless File.exists? path
        uri = URI(url)
        req = Net::HTTP::Get.new(uri)
        headers&.each { |k,v| req[k] = v }
        res = Net::HTTP.start(uri.hostname, uri.port, :use_ssl => (uri.scheme == 'https')) { |http|
          http.request(req)
        }
        if res.is_a?(Net::HTTPSuccess)
          data = res.body
          File.open(path, 'w') do |f|
            f.write data
          end
        elsif res.is_a?(Net::HTTPRedirection)
          puts "#{res.code}: #{res.message} for #{url} (will retry redirect)"
          return cached_request(res['location'], headers, limit - 1)
        else
          puts "#{res.code}: #{res.message} for #{url}"
          return {}
        end
      end
      JSON.parse(File.read(path))
    end

    # Fetch stats and other interesting data from the GitHub API
    def get_repo_data(repo)
      headers = {'Authorization': "token #{GH_OAUTH_TOKEN}"} if GH_OAUTH_TOKEN
      data = cached_request("https://api.github.com/repos/#{repo}", headers)
      unless data.empty?
        branch = data['default_branch'] || 'master'
        commit = cached_request("https://api.github.com/repos/#{repo}/commits/#{branch}", headers)
        contributors = cached_request("https://api.github.com/repos/#{repo}/contributors", headers)
      end

      out = {}
      %w(stargazers_count open_issues_count).each do |k|
        out[k] ||= data[k]
      end
      out['last_commit'] = commit&.dig('commit', 'committer', 'date')
      out['contributor_count'] = contributors&.length
      out.delete_if { |k,v| v.nil? }
    end

    # Fetch stats and other interesting data from the crates.io API
    def get_crate_data(crate)
      data = cached_request("https://crates.io/api/v1/crates/#{crate}")

      out = {}
      %w(description repository documentation downloads license max_version created_at updated_at).each do |k|
        out[k] = data.dig('crate', k)
      end
      out.delete_if { |k,v| v.nil? }
    end

    # Save all the collected data to crates_generated.yaml
    def save_crate_cache(crates)
      puts "Saving crate cache..."
      FileUtils.mkdir_p(File.join(__dir__, "../_tmp"))
      File.open(File.join(__dir__, "../_tmp/crates_generated.yaml"), 'w') do |f|
        f.write "# This file is generated - do not edit it manually\n\n"
        f.write crates.to_yaml
      end
    end

    def read_crate_cache
      YAML.load_file(File.join(__dir__, "../_tmp/crates_generated.yaml"))
    end

  end

end
